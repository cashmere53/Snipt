//! Where the coordinator runs its slow work (screen-capture 4.1): freezing the monitors,
//! capturing a window, composing and the post-capture flow. Callers (hotkeys, overlay
//! commands) return at once.

use std::io;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

pub type Job = Box<dyn FnOnce() + Send + 'static>;

pub trait CaptureWorker: Send + Sync {
    /// Runs `job` after the jobs already given, in order.
    fn run(&self, job: Job);
    /// Runs `job` once `delay` has passed.
    fn run_after(&self, delay: Duration, job: Job);
}

/// One `capture-worker` thread taking jobs in order.
pub struct ThreadWorker {
    jobs: Sender<Job>,
}

impl ThreadWorker {
    /// Starts the thread; `init` runs on it first (thread DPI awareness, COM).
    pub fn spawn(init: impl FnOnce() + Send + 'static) -> io::Result<Self> {
        let (jobs, queue) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("capture-worker".to_owned())
            .spawn(move || {
                init();
                for job in queue {
                    job();
                }
            })?;
        Ok(Self { jobs })
    }
}

impl CaptureWorker for ThreadWorker {
    fn run(&self, job: Job) {
        if self.jobs.send(job).is_err() {
            log::error!("capture: the capture-worker thread has stopped");
        }
    }

    fn run_after(&self, delay: Duration, job: Job) {
        let jobs = self.jobs.clone();
        let timer = thread::Builder::new()
            .name("capture-timer".to_owned())
            .spawn(move || {
                thread::sleep(delay);
                // The worker has stopped only when the app is quitting.
                let _ = jobs.send(job);
            });
        if let Err(error) = timer {
            log::error!("capture: could not start a timer: {error}");
        }
    }
}

#[cfg(test)]
pub mod fake {
    use std::sync::Mutex;

    use super::*;

    /// Runs jobs at once on the caller's thread, or keeps them for `run_held` when made with
    /// `holding`; delayed jobs wait for `run_delayed`.
    #[derive(Default)]
    pub struct InlineWorker {
        hold: bool,
        held: Mutex<Vec<Job>>,
        delayed: Mutex<Vec<(Duration, Job)>>,
    }

    impl InlineWorker {
        /// A worker that runs nothing until `run_held`: a busy worker thread.
        pub fn holding() -> Self {
            Self {
                hold: true,
                ..Self::default()
            }
        }

        /// Runs the held jobs in order.
        pub fn run_held(&self) {
            let jobs = std::mem::take(&mut *self.held.lock().unwrap());
            for job in jobs {
                job();
            }
        }

        /// The delays of the jobs waiting.
        pub fn delays(&self) -> Vec<Duration> {
            self.delayed
                .lock()
                .unwrap()
                .iter()
                .map(|(d, _)| *d)
                .collect()
        }

        /// Runs the delayed jobs in the order given, as if their time had come.
        pub fn run_delayed(&self) {
            let jobs = std::mem::take(&mut *self.delayed.lock().unwrap());
            for (_, job) in jobs {
                job();
            }
        }
    }

    impl CaptureWorker for InlineWorker {
        fn run(&self, job: Job) {
            if self.hold {
                self.held.lock().unwrap().push(job);
            } else {
                job();
            }
        }

        fn run_after(&self, delay: Duration, job: Job) {
            self.delayed.lock().unwrap().push((delay, job));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn runs_jobs_in_order_on_the_named_thread_after_init() {
        let (tx, rx) = mpsc::channel();
        let init_tx = tx.clone();
        let worker = ThreadWorker::spawn(move || init_tx.send("init".to_owned()).unwrap()).unwrap();
        for i in 0..3 {
            let tx = tx.clone();
            worker.run(Box::new(move || {
                let name = thread::current().name().unwrap().to_owned();
                tx.send(format!("{name} {i}")).unwrap();
            }));
        }
        let later = tx.clone();
        worker.run_after(
            Duration::from_millis(10),
            Box::new(move || later.send("later".to_owned()).unwrap()),
        );
        let got: Vec<String> = rx.iter().take(5).collect();
        assert_eq!(
            got,
            [
                "init",
                "capture-worker 0",
                "capture-worker 1",
                "capture-worker 2",
                "later"
            ]
        );
    }
}
