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
                        let result_sender = task.result_sender; // Use it directly

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

                        // Send the result
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
