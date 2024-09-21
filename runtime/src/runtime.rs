use std::{collections::VecDeque, process::Command, sync::Arc};

use eyre::Result;
use futures::channel::oneshot;
use parking_lot::Mutex;
use reth_exex_types::ExExNotification;
use std::thread;
use tracing::info;

#[derive(Debug)]
pub struct JoinHandle<T>(oneshot::Receiver<T>);

impl<T> std::future::Future for JoinHandle<T> {
    type Output = Result<T, oneshot::Canceled>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.0).poll(cx)
    }
}

struct Task {
    binary_path: String,
    exex_notification: Arc<ExExNotification>,
    result_sender: oneshot::Sender<Result<()>>,
}

pub struct Runtime {
    inner: Arc<Inner>,
}

struct Inner {
    tasks: Mutex<VecDeque<Task>>,
    workers: Mutex<Vec<thread::Thread>>,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        let tasks = Mutex::new(VecDeque::new());
        let workers = Mutex::new(Vec::with_capacity(thread::available_parallelism()?.get()));

        let inner = Arc::new(Inner { workers, tasks });

        for _ in 0..thread::available_parallelism()?.get() {
            let inner = Arc::clone(&inner);

            thread::spawn(move || {
                loop {
                    while let Some(task) = inner.tasks.lock().pop_front() {
                        let binary_path = task.binary_path.clone();
                        let exex_notification = task.exex_notification.clone();
                        let result_sender = task.result_sender.clone();

                        let status = Command::new(binary_path)
                            .arg(serde_json::to_string(&*exex_notification).unwrap())
                            .status();

                        let res = match status {
                            Ok(status) if status.success() => Ok(()),
                            Err(e) => Err(eyre::eyre!("Failed to execute binary: {}", e)),
                            _ => Err(eyre::eyre!(
                                "Binary execution failed with status: {:?}",
                                status
                            )),
                        };

                        let _ = result_sender.send(res);
                    }

                    // Park the thread if no tasks
                    let handle = thread::current();
                    inner.workers.lock().push(handle);
                    thread::park();
                }
            });
        }

        Ok(Self { inner })
    }

    pub fn spawn(
        &self,
        binary_path: String,
        exex_notification: Arc<ExExNotification>,
    ) -> JoinHandle<Result<()>> {
        let (result_sender, rx) = oneshot::channel();

        // Create task
        let task = Task { binary_path, exex_notification, result_sender };
        self.inner.tasks.lock().push_back(task);

        // Wake up available worker
        self.wake();

        JoinHandle(rx)
    }

    fn wake(&self) {
        if let Some(worker) = self.inner.workers.lock().pop() {
            worker.unpark();
        }
    }
}
