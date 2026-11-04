// SPDX-License-Identifier: AGPL-3.0-or-later
//! Own the child process and bound a request across both pipe directions.
use super::{
    Manifest, ipc,
    protocol::{Operation, Reply, Request, ResultBody, VERSION},
};
use bbsbox_maintenance::{
    Error as MaintenanceError,
    checkpoints::{WorkerArtifact, WorkerCheckpoint},
};
use rand_core::{OsRng, RngCore};
use std::{
    io,
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
    child: Child,
    queue: SyncSender<Work>,
    timeout: Duration,
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
        let mut input = child.stdin.take().expect("piped worker input");
        let mut output = child.stdout.take().expect("piped worker output");
        let (queue, jobs) = mpsc::sync_channel::<Work>(1);
        if let Err(error) = std::thread::Builder::new()
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
            })
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let mut worker = Self {
            child,
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
            self.stop();
        }
        result
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
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
