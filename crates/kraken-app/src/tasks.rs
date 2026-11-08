//! Background work, in one place.
//!
//! - [`spawn`] runs a short job (git status, a notification, `open`) on a small
//!   shared pool of worker threads.
//! - [`spawn_long`] gives a job that lives a long time (a timer loop, waiting
//!   for an editor to exit) its own named thread, so it never ties up the pool.
//!
//! Either way a panicking job is caught and logged: it can't take a worker,
//! or the app, down with it.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};

type Job = (&'static str, Box<dyn FnOnce() + Send>);

const WORKERS: usize = 4;

fn pool() -> &'static Mutex<Sender<Job>> {
    static POOL: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        let rx = std::sync::Arc::new(Mutex::new(rx));
        for i in 0..WORKERS {
            let rx = rx.clone();
            let spawned = std::thread::Builder::new().name(format!("kraken-worker-{i}")).spawn(move || {
                loop {
                    // Hold the lock only while taking a job, not while running it.
                    let job = match rx.lock() {
                        Ok(rx) => rx.recv(),
                        Err(poisoned) => poisoned.into_inner().recv(),
                    };
                    let Ok((label, job)) = job else { return };
                    run(label, job);
                }
            });
            if let Err(e) = spawned {
                log::error!("could not start worker thread: {e}");
            }
        }
        Mutex::new(tx)
    })
}

fn run(label: &str, job: impl FnOnce()) {
    if let Err(panic) = catch_unwind(AssertUnwindSafe(job)) {
        let msg = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        log::error!("background task {label:?} panicked: {msg}");
    }
}

/// Run a short job on the shared worker pool.
pub fn spawn(label: &'static str, job: impl FnOnce() + Send + 'static) {
    let sent = match pool().lock() {
        Ok(tx) => tx.send((label, Box::new(job))),
        Err(poisoned) => poisoned.into_inner().send((label, Box::new(job))),
    };
    if let Err(e) = sent {
        // The pool is gone (only during shutdown): run it right here.
        let (label, job) = e.0;
        run(label, job);
    }
}

/// Run a long-lived job on its own named thread.
pub fn spawn_long(name: &str, job: impl FnOnce() + Send + 'static) {
    let label = name.to_string();
    if let Err(e) = std::thread::Builder::new().name(format!("kraken-{name}")).spawn(move || run(&label, job)) {
        log::error!("could not start thread {name:?}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn jobs_run_and_panics_do_not_kill_the_pool() {
        for _ in 0..WORKERS * 2 {
            spawn("boom", || panic!("expected in test"));
        }
        let (tx, rx) = channel();
        for i in 0..20 {
            let tx = tx.clone();
            spawn("count", move || tx.send(i).unwrap());
        }
        let mut got: Vec<i32> = (0..20).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        got.sort();
        assert_eq!(got, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn long_jobs_get_their_own_thread() {
        let (tx, rx) = channel();
        spawn_long("test-long", move || tx.send(std::thread::current().name().map(String::from)).unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().as_deref(), Some("kraken-test-long"));
    }
}
