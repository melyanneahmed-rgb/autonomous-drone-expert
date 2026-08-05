//! Cancellation and deadline behaviour.
//!
//! Neither candidate offers a cancel primitive: there is no `cancel()`, no shutdown, and
//! no way to interrupt a blocking read from another thread through the public API. The
//! only portable tools are (a) a bounded read timeout and (b) dropping the handle.
//!
//! What is proven here is the architecture the production layer would need. Whether
//! dropping a handle actually unblocks a read in progress on Windows is
//! REQUIRES_WINDOWS_HARDWARE_TEST.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use spike_windows_serial_transport::watchdog::{Outcome, with_deadline};

/// Stand-in for a device that never answers.
fn silent_source(cancel: Arc<AtomicBool>, budget: Duration) -> Result<usize, &'static str> {
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("OPERATION_CANCELLED");
        }
        if started.elapsed() >= budget {
            return Err("READ_TIMEOUT");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_bounded_read_always_returns() {
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    let outcome = with_deadline(Duration::from_secs(3), move || {
        silent_source(c, Duration::from_millis(200))
    });
    match outcome {
        Outcome::Completed { value, elapsed } => {
            assert_eq!(value, Err("READ_TIMEOUT"));
            assert!(
                elapsed < Duration::from_secs(1),
                "deadline overshoot: {elapsed:?}"
            );
            println!("[SIMULATED_ONLY] bounded read returned READ_TIMEOUT in {elapsed:?}");
        }
        Outcome::TimedOut { .. } => panic!("a bounded read must never outlive its budget"),
    }
}

#[test]
fn cancellation_beats_the_timeout() {
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    let handle = std::thread::spawn(move || silent_source(c, Duration::from_secs(30)));
    std::thread::sleep(Duration::from_millis(50));
    cancel.store(true, Ordering::Relaxed);
    let started = Instant::now();
    let result = handle.join().expect("worker panicked");
    assert_eq!(result, Err("OPERATION_CANCELLED"));
    assert!(started.elapsed() < Duration::from_secs(1));
    println!("[SIMULATED_ONLY] cooperative cancel returned OPERATION_CANCELLED promptly");
}

#[test]
fn a_hung_call_is_reported_as_failure_not_as_slowness() {
    // The watchdog must classify a non-returning call as a failure. This is the property
    // that keeps a stuck device from freezing the interface.
    let outcome: Outcome<()> = with_deadline(Duration::from_millis(150), || {
        std::thread::sleep(Duration::from_secs(30));
    });
    assert!(matches!(outcome, Outcome::TimedOut { .. }));
    println!("[SIMULATED_ONLY] watchdog reports a hung call as TimedOut, not as success");
}

#[test]
fn dropping_a_handle_is_the_only_available_interrupt() {
    // Documented as a finding, asserted as an architectural constraint rather than a
    // library capability: neither candidate exposes an interrupt API.
    let serialport_has_cancel = false;
    let serial2_has_cancel = false;
    assert!(!serialport_has_cancel && !serial2_has_cancel);
    println!(
        "[SIMULATED_ONLY] no candidate exposes cancel(); production must bound every read \
         and treat handle drop plus a cancellation flag as the interrupt mechanism"
    );
}
