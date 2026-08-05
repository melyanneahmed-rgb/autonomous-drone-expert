//! Deadline harness.
//!
//! Every candidate call in this spike runs under a watchdog, because the failure this
//! project cannot tolerate is not an error — it is a call that never returns and freezes
//! the interface. A hung call is reported as a failure, not as a slow success.
//!
//! **What this is not:** cancellation. The watchdog stops *waiting* for a hung call; it
//! does not and cannot stop the call. The worker thread keeps running. Whether anything
//! can interrupt a real blocking read on Windows (for example by dropping the handle
//! from another thread) is `REQUIRES_WINDOWS_HARDWARE_TEST` and remains unproven for
//! both candidates.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome<T> {
    Completed {
        value: T,
        elapsed: Duration,
    },
    /// The call did not return within the deadline. The worker thread is deliberately
    /// left running: there is no safe way to kill it, which is itself a finding.
    TimedOut {
        deadline: Duration,
    },
}

impl<T> Outcome<T> {
    pub fn completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }
}

/// Run `f` on a worker thread and give up after `deadline`.
pub fn with_deadline<T, F>(deadline: Duration, f: F) -> Outcome<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    thread::spawn(move || {
        let value = f();
        let _ = tx.send(value);
    });
    match rx.recv_timeout(deadline) {
        Ok(value) => Outcome::Completed {
            value,
            elapsed: started.elapsed(),
        },
        Err(_) => Outcome::TimedOut { deadline },
    }
}
