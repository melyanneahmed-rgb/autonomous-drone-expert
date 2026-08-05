#![forbid(unsafe_code)]

//! Complete-write semantics — SIMULATED_ONLY.
//!
//! No byte here reaches any device. The transport side is a scripted simulator and the
//! clock is injected, so the timeout and disconnect paths run deterministically. What a
//! real Windows COM port does under partial writes remains
//! REQUIRES_WINDOWS_HARDWARE_TEST.

use std::time::Duration;

use spike_windows_serial_transport::TransportError;
use spike_windows_serial_transport::contract::{WriteAllFailure, write_all_with_deadline_impl};

/// Scripted transport: each entry is the next `write_some` result.
struct Script {
    steps: Vec<Result<usize, TransportError>>,
    calls: usize,
    accepted: usize,
}

impl Script {
    fn new(steps: Vec<Result<usize, TransportError>>) -> Self {
        Self {
            steps,
            calls: 0,
            accepted: 0,
        }
    }

    fn write_some(&mut self, offered: &[u8]) -> Result<usize, TransportError> {
        let step = self.steps.get(self.calls).copied().unwrap_or(Ok(0));
        self.calls += 1;
        if let Ok(n) = step {
            assert!(n <= offered.len(), "simulator accepted more than offered");
            self.accepted += n;
        }
        step
    }
}

fn ticking_clock(step_ms: u64) -> impl FnMut() -> Duration {
    let mut ticks = 0u64;
    move || {
        let now = Duration::from_millis(ticks * step_ms);
        ticks += 1;
        now
    }
}

const DEADLINE: Duration = Duration::from_millis(1000);

#[test]
fn partial_write_then_completion_succeeds_only_when_all_bytes_are_written() {
    let payload = [0u8; 10];
    let mut sim = Script::new(vec![Ok(4), Ok(6)]);
    let result =
        write_all_with_deadline_impl(|b| sim.write_some(b), &payload, DEADLINE, ticking_clock(1));
    assert_eq!(result, Ok(10));
    assert_eq!(sim.accepted, 10);
    println!("[SIMULATED_ONLY] 4 then 6 bytes -> success declared only at 10/10");
}

#[test]
fn many_partial_writes_accumulate_correctly() {
    let payload = [0u8; 9];
    let mut sim = Script::new(vec![Ok(1), Ok(3), Ok(0), Ok(2), Ok(3)]);
    let result =
        write_all_with_deadline_impl(|b| sim.write_some(b), &payload, DEADLINE, ticking_clock(1));
    assert_eq!(result, Ok(9));
    assert_eq!(sim.calls, 5);
    println!("[SIMULATED_ONLY] 1+3+0+2+3 partial writes -> 9/9, zero-progress step tolerated");
}

#[test]
fn zero_byte_payload_is_complete_without_touching_the_transport() {
    let mut sim = Script::new(vec![]);
    let result =
        write_all_with_deadline_impl(|b| sim.write_some(b), &[], DEADLINE, ticking_clock(1));
    assert_eq!(result, Ok(0));
    assert_eq!(
        sim.calls, 0,
        "the transport must not be called for an empty payload"
    );
    println!("[SIMULATED_ONLY] zero-byte write completes without a transport call");
}

#[test]
fn timeout_before_completion_reports_bytes_written_and_write_timeout() {
    let payload = [0u8; 10];
    // Accepts 3 bytes, then stalls forever with zero-progress writes.
    let mut sim = Script::new(vec![Ok(3)]);
    let result = write_all_with_deadline_impl(
        |b| sim.write_some(b),
        &payload,
        Duration::from_millis(5),
        ticking_clock(1),
    );
    assert_eq!(
        result,
        Err(WriteAllFailure {
            bytes_written: 3,
            error: TransportError::WriteTimeout
        })
    );
    println!(
        "[SIMULATED_ONLY] stall after 3/10 -> WRITE_TIMEOUT with bytes_written=3, no success claim"
    );
}

#[test]
fn disconnect_mid_write_reports_progress_and_disconnection() {
    let payload = [0u8; 10];
    let mut sim = Script::new(vec![Ok(4), Err(TransportError::DeviceDisconnected)]);
    let result =
        write_all_with_deadline_impl(|b| sim.write_some(b), &payload, DEADLINE, ticking_clock(1));
    assert_eq!(
        result,
        Err(WriteAllFailure {
            bytes_written: 4,
            error: TransportError::DeviceDisconnected
        })
    );
    println!("[SIMULATED_ONLY] disconnect after 4/10 -> DEVICE_DISCONNECTED with bytes_written=4");
}

#[test]
fn success_is_never_declared_below_full_length() {
    let payload = [0u8; 8];
    for steps in [vec![Ok(7)], vec![Ok(3), Ok(4)], vec![Ok(0)]] {
        let mut sim = Script::new(steps);
        let result = write_all_with_deadline_impl(
            |b| sim.write_some(b),
            &payload,
            Duration::from_millis(4),
            ticking_clock(1),
        );
        match result {
            Ok(n) => panic!("declared success at {n}/8 without completing the payload"),
            Err(f) => assert!(f.bytes_written < 8),
        }
    }
    println!("[SIMULATED_ONLY] no path declares success below 8/8 bytes");
}
