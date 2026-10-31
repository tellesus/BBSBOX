// SPDX-License-Identifier: AGPL-3.0-or-later
//! One bounded writer thread; callers await replies without running SQLite on async threads.
use crate::{
    Database, Error, Migration, Result,
    clock::{Clock, SystemClock},
};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions, TryLockError},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock, mpsc},
    thread::{self, JoinHandle, ThreadId},
};
use tokio::sync::oneshot;

#[derive(Clone, Copy)]
pub enum Priority {
    Interactive,
    Background,
}
#[derive(Clone, Copy)]
pub struct Config {
    pub capacity: usize,
    pub interactive_burst: usize,
    pub disk: crate::disk::Budget,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            capacity: 64,
            interactive_burst: 8,
            disk: crate::disk::Budget::default(),
        }
    }
}
type Task = Box<dyn FnOnce(&mut Database) + Send + 'static>;
struct Queues {
    interactive: VecDeque<Task>,
    background: VecDeque<Task>,
    stopping: bool,
}
struct Shared {
    queues: Mutex<Queues>,
    ready: Condvar,
    thread: OnceLock<ThreadId>,
    config: Config,
}
#[derive(Clone)]
pub struct Handle {
    shared: Arc<Shared>,
}
pub struct Pending<T> {
    reply: oneshot::Receiver<Result<T>>,
}
impl<T> Pending<T> {
    pub async fn wait(self) -> Result<T> {
        self.reply.await.map_err(|_| Error::WriterClosed)?
    }
    /// Only for non-async callers. Async request handlers use wait().await.
    pub fn blocking_wait(self) -> Result<T> {
        self.reply
            .blocking_recv()
            .map_err(|_| Error::WriterClosed)?
    }
}
pub struct Writer {
    handle: Handle,
    worker: Option<JoinHandle<()>>,
}
fn lock(path: &Path) -> Result<(File, PathBuf)> {
    let path = if path.exists() {
        path.canonicalize()?
    } else {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        parent
            .canonicalize()?
            .join(path.file_name().ok_or(Error::InvalidConfiguration)?)
    };
    let mut name = path.as_os_str().to_os_string();
    name.push(".writer.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(name)?;
    match file.try_lock() {
        Ok(()) => Ok((file, path)),
        Err(TryLockError::WouldBlock) => Err(Error::WriterAlreadyRunning),
        Err(TryLockError::Error(error)) => Err(Error::Io(error)),
    }
}
impl Writer {
    pub fn start(
        path: impl AsRef<Path>,
        migrations: Vec<Migration>,
        config: Config,
    ) -> Result<Self> {
        Self::start_with_clock(path, migrations, config, Arc::new(SystemClock::default()))
    }
    pub fn start_with_clock(
        path: impl AsRef<Path>,
        migrations: Vec<Migration>,
        config: Config,
        clock: Arc<dyn Clock>,
    ) -> Result<Self> {
        Self::start_with_resources(
            path,
            migrations,
            config,
            clock,
            Arc::new(crate::disk::FilesystemSpace),
        )
    }
    pub fn start_with_resources(
        path: impl AsRef<Path>,
        migrations: Vec<Migration>,
        config: Config,
        clock: Arc<dyn Clock>,
        space: Arc<dyn crate::disk::SpaceProbe>,
    ) -> Result<Self> {
        if config.capacity == 0 || config.interactive_burst == 0 {
            return Err(Error::InvalidConfiguration);
        }
        config.disk.validate()?;
        let (file, path) = lock(path.as_ref())?;
        if crate::snapshot::recovery_marker(&path).exists() {
            return Err(Error::RecoveryRequired);
        }
        let shared = Arc::new(Shared {
            queues: Mutex::new(Queues {
                interactive: VecDeque::new(),
                background: VecDeque::new(),
                stopping: false,
            }),
            ready: Condvar::new(),
            thread: OnceLock::new(),
            config,
        });
        let handle = Handle {
            shared: shared.clone(),
        };
        let (ready, started) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("bbsbox-storage-writer".into())
            .spawn(move || {
                let _file = file;
                let _ = shared.thread.set(thread::current().id());
                let database =
                    Database::open_with_clock(path, &migrations, clock).and_then(|mut database| {
                        database.configure_disk(config.disk, space)?;
                        database.recover_running_jobs()?;
                        Ok(database)
                    });
                let mut database = match database {
                    Ok(database) => {
                        let _ = ready.send(Ok(()));
                        database
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                let mut interactive_run = 0;
                loop {
                    let task = {
                        let mut queues = shared.queues.lock().unwrap();
                        while queues.interactive.is_empty()
                            && queues.background.is_empty()
                            && !queues.stopping
                        {
                            queues = shared.ready.wait(queues).unwrap();
                        }
                        if queues.interactive.is_empty()
                            && queues.background.is_empty()
                            && queues.stopping
                        {
                            break;
                        }
                        if !queues.background.is_empty()
                            && (queues.interactive.is_empty()
                                || interactive_run >= config.interactive_burst)
                        {
                            interactive_run = 0;
                            queues.background.pop_front()
                        } else {
                            interactive_run = interactive_run.saturating_add(1);
                            queues.interactive.pop_front()
                        }
                    };
                    if let Some(task) = task {
                        task(&mut database);
                    }
                }
            })?;
        match started.recv() {
            Ok(Ok(())) => Ok(Self {
                handle,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = worker.join();
                Err(Error::WriterClosed)
            }
        }
    }
    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }
    /// Closes admission and drains accepted work. Call from a shutdown/blocking context.
    pub fn shutdown(mut self) -> Result<()> {
        self.stop()
    }
    fn stop(&mut self) -> Result<()> {
        {
            let mut queues = self
                .handle
                .shared
                .queues
                .lock()
                .map_err(|_| Error::WriterClosed)?;
            queues.stopping = true;
            self.handle.shared.ready.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            if worker.thread().id() == thread::current().id() {
                return Err(Error::ReentrantWriter);
            }
            worker.join().map_err(|_| Error::WriterClosed)?;
        }
        Ok(())
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
impl Handle {
    pub fn submit<T: Send + 'static>(
        &self,
        priority: Priority,
        work: impl FnOnce(&mut Database) -> Result<T> + Send + 'static,
    ) -> Result<Pending<T>> {
        if self
            .shared
            .thread
            .get()
            .is_some_and(|id| *id == thread::current().id())
        {
            return Err(Error::ReentrantWriter);
        }
        let mut queues = self.shared.queues.lock().map_err(|_| Error::WriterClosed)?;
        if queues.stopping {
            return Err(Error::WriterClosed);
        }
        if queues.interactive.len() + queues.background.len() >= self.shared.config.capacity {
            return Err(Error::QueueFull);
        }
        let (reply, receiver) = oneshot::channel();
        let task: Task = Box::new(move |database| {
            let result = catch_unwind(AssertUnwindSafe(|| work(database)))
                .unwrap_or(Err(Error::WorkPanicked));
            let _ = reply.send(result); // A disconnected caller does not cancel accepted durable work.
        });
        match priority {
            Priority::Interactive => queues.interactive.push_back(task),
            Priority::Background => queues.background.push_back(task),
        };
        drop(queues);
        self.shared.ready.notify_one();
        Ok(Pending { reply: receiver })
    }
}
