// SPDX-License-Identifier: AGPL-3.0-or-later
//! Own a local worker connection and bound each framed request.
use super::{
    Manifest, ipc,
    protocol::{Operation, Reply, Request, ResultBody, VERSION},
};
use bbsbox_maintenance::{
    Error as MaintenanceError,
    checkpoints::{WorkerArtifact, WorkerCheckpoint},
};
use rand_core::{OsRng, RngCore};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::{
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, SyncSender},
    time::Duration,
};

struct Work {
    request: Request,
    response: SyncSender<io::Result<Reply>>,
}

pub struct Worker {
    transport: Option<Transport>,
    queue: SyncSender<Work>,
    timeout: Duration,
}

enum Transport {
    Child(Child),
    #[cfg(unix)]
    Socket(UnixStream),
}

impl Drop for Transport {
    fn drop(&mut self) {
        match self {
            Self::Child(child) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            #[cfg(unix)]
            Self::Socket(socket) => {
                let _ = socket.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

impl Worker {
    /// The caller selects an installed executable and the worker's own data path.
    pub fn launch(
        executable: &Path,
        state: &Path,
        expected: &Manifest,
        timeout: Duration,
    ) -> io::Result<Self> {
        Self::launch_mode(executable, state, expected, timeout, false)
    }

    pub fn launch_checkpoint(
        executable: &Path,
        state: &Path,
        expected: &Manifest,
        timeout: Duration,
    ) -> io::Result<Self> {
        Self::launch_mode(executable, state, expected, timeout, true)
    }

    /// The appliance supplies a protected filesystem socket to its separate worker account.
    #[cfg(unix)]
    pub fn connect(path: &Path, expected: &Manifest, timeout: Duration) -> io::Result<Self> {
        use socket2::{Domain, SockAddr, Socket, Type};
        if !path.is_absolute() || !expected.valid() || timeout.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid game worker socket configuration",
            ));
        }
        let socket = Socket::new(Domain::UNIX, Type::STREAM, None)?;
        socket.set_nonblocking(true)?;
        socket.connect(&SockAddr::unix(path)?)?;
        socket.set_nonblocking(false)?;
        let socket = UnixStream::from(socket);
        Self::start(
            Transport::Socket(socket.try_clone()?),
            socket.try_clone()?,
            socket,
            expected,
            timeout,
        )
    }

    fn launch_mode(
        executable: &Path,
        state: &Path,
        expected: &Manifest,
        timeout: Duration,
        checkpoint_only: bool,
    ) -> io::Result<Self> {
        if !executable.is_absolute()
            || !state.is_absolute()
            || !expected.valid()
            || timeout.is_zero()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid local game launch configuration",
            ));
        }
        let mut child = Command::new(executable)
            .arg(if checkpoint_only {
                "--checkpoint-state"
            } else {
                "--state"
            })
            .arg(state)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().expect("piped worker input");
        let output = child.stdout.take().expect("piped worker output");
        Self::start(Transport::Child(child), input, output, expected, timeout)
    }

    fn start(
        transport: Transport,
        mut input: impl Write + Send + 'static,
        mut output: impl Read + Send + 'static,
        expected: &Manifest,
        timeout: Duration,
    ) -> io::Result<Self> {
        let (queue, jobs) = mpsc::sync_channel::<Work>(1);
        std::thread::Builder::new()
            .name("bbsbox-game-ipc".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let result = ipc::send(&mut input, &job.request).and_then(|()| {
                        ipc::receive::<Reply>(&mut output)?.ok_or_else(|| {
                            io::Error::new(io::ErrorKind::UnexpectedEof, "game worker closed")
                        })
                    });
                    let healthy = result.is_ok();
                    let _ = job.response.send(result);
                    if !healthy {
                        break;
                    }
                }
            })?;
        let mut worker = Self {
            transport: Some(transport),
            queue,
            timeout,
        };
        match worker.call(Operation::Describe)? {
            ResultBody::Hello { manifest } if manifest == *expected => Ok(worker),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "game worker manifest mismatch",
            )),
        }
    }

    pub fn call(&mut self, operation: Operation) -> io::Result<ResultBody> {
        let mut id = [0; 16];
        OsRng.fill_bytes(&mut id);
        let (response, receiver) = mpsc::sync_channel(1);
        self.queue
            .try_send(Work {
                request: Request {
                    version: VERSION,
                    id,
                    operation,
                },
                response,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "game worker is unavailable"))?;
        let result = receiver
            .recv_timeout(self.timeout)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => {
                    io::Error::new(io::ErrorKind::TimedOut, "game worker timed out")
                }
                mpsc::RecvTimeoutError::Disconnected => {
                    io::Error::new(io::ErrorKind::BrokenPipe, "game worker is unavailable")
                }
            })
            .and_then(|result| result)
            .and_then(|reply| {
                if reply.id != id || reply.version != VERSION {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "uncorrelated game response",
                    ));
                }
                Ok(reply.result)
            });
        if result.is_err() {
            drop(self.transport.take());
        }
        result
    }
}

impl WorkerCheckpoint for Worker {
    fn pause(&mut self, request: [u8; 16]) -> bbsbox_maintenance::Result<()> {
        match self.call(Operation::Pause { fence: request }) {
            Ok(ResultBody::Paused { fence }) if fence == request => Ok(()),
            _ => Err(MaintenanceError::CheckpointUnavailable),
        }
    }

    fn checkpoint(&mut self, request: [u8; 16]) -> bbsbox_maintenance::Result<WorkerArtifact> {
        match self.call(Operation::Checkpoint { fence: request }) {
            Ok(ResultBody::Checkpoint {
                fence,
                checkpoint_id,
                state_schema,
                worker_version,
                directory,
            }) if fence == request => Ok(WorkerArtifact {
                request: fence,
                checkpoint_id,
                state_schema,
                worker_version,
                directory,
            }),
            _ => Err(MaintenanceError::CheckpointUnavailable),
        }
    }

    fn resume(&mut self, request: [u8; 16]) -> bbsbox_maintenance::Result<()> {
        match self.call(Operation::Resume { fence: request }) {
            Ok(ResultBody::Resumed { fence }) if fence == request => Ok(()),
            _ => Err(MaintenanceError::CheckpointUnavailable),
        }
    }
}
