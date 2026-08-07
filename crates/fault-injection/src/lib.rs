#![forbid(unsafe_code)]

//! # `ade-fault-injection` — deterministic fault injection (M1)
//!
//! Wraps any [`FrameResponder`] and, at a chosen request ordinal, injects one deterministic
//! fault instead of (or on top of) the real reply. Every other request is delegated
//! unchanged. This drives the mandatory failure scenarios without any real hardware.

use ade_transport::{FrameResponder, TransportError};

/// A single deterministic fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// A read timeout.
    Timeout,
    /// A request that never gets a reply.
    NoReply,
    /// The device disconnected.
    Disconnected,
    /// The port is busy.
    PortBusy,
    /// Permission denied.
    PermissionDenied,
    /// No driver.
    MissingDriver,
    /// A reply that does not decode as a frame at all.
    CorruptFrame,
    /// A reply whose checksum byte is wrong.
    BadChecksum,
}

impl Fault {
    /// The transport error this fault surfaces directly, if it is an error-shaped fault.
    /// Frame-shaped faults ([`Fault::CorruptFrame`], [`Fault::BadChecksum`]) return `None`
    /// because they produce a (broken) reply rather than an error.
    #[must_use]
    pub const fn as_error(self) -> Option<TransportError> {
        match self {
            Fault::Timeout => Some(TransportError::Timeout),
            Fault::NoReply => Some(TransportError::NoReply),
            Fault::Disconnected => Some(TransportError::Disconnected),
            Fault::PortBusy => Some(TransportError::PortBusy),
            Fault::PermissionDenied => Some(TransportError::PermissionDenied),
            Fault::MissingDriver => Some(TransportError::MissingDriver),
            Fault::CorruptFrame | Fault::BadChecksum => None,
        }
    }
}

/// Injects one [`Fault`] at a chosen zero-based request ordinal; delegates everything else.
#[derive(Debug)]
pub struct FaultInjector<R: FrameResponder> {
    inner: R,
    at: usize,
    fault: Fault,
    calls: usize,
}

impl<R: FrameResponder> FaultInjector<R> {
    /// Inject `fault` on the request at index `at` (0-based); all others pass through.
    pub fn new(inner: R, at: usize, fault: Fault) -> Self {
        Self {
            inner,
            at,
            fault,
            calls: 0,
        }
    }

    /// How many requests have been handled so far.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls
    }
}

impl<R: FrameResponder> FrameResponder for FaultInjector<R> {
    fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        let fire = self.calls == self.at;
        self.calls += 1;
        if !fire {
            return self.inner.respond(request);
        }
        if let Some(error) = self.fault.as_error() {
            return Err(error);
        }
        match self.fault {
            // A reply with no `$M` preamble: it will not decode.
            Fault::CorruptFrame => Ok(vec![0x00, 0x01, 0x02, 0x03]),
            // The real reply with its checksum flipped.
            Fault::BadChecksum => {
                let mut bytes = self.inner.respond(request)?;
                if let Some(last) = bytes.last_mut() {
                    *last ^= 0xFF;
                }
                Ok(bytes)
            }
            _ => unreachable!("error-shaped faults handled above"),
        }
    }
}

/// Injects faults at several chosen request ordinals (0-based); delegates everything else.
/// Used by compound failure scenarios (e.g. a save failure followed by a recovery failure).
#[derive(Debug)]
pub struct ScheduledFaultInjector<R: FrameResponder> {
    inner: R,
    schedule: Vec<(usize, Fault)>,
    calls: usize,
}

impl<R: FrameResponder> ScheduledFaultInjector<R> {
    /// Inject each `(ordinal, fault)` pair at its 0-based request ordinal.
    pub fn new(inner: R, schedule: Vec<(usize, Fault)>) -> Self {
        Self {
            inner,
            schedule,
            calls: 0,
        }
    }

    /// How many requests have been handled so far.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls
    }
}

impl<R: FrameResponder> FrameResponder for ScheduledFaultInjector<R> {
    fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        let ordinal = self.calls;
        self.calls += 1;
        let Some(&(_, fault)) = self.schedule.iter().find(|(at, _)| *at == ordinal) else {
            return self.inner.respond(request);
        };
        if let Some(error) = fault.as_error() {
            return Err(error);
        }
        match fault {
            Fault::CorruptFrame => Ok(vec![0x00, 0x01, 0x02, 0x03]),
            Fault::BadChecksum => {
                let mut bytes = self.inner.respond(request)?;
                if let Some(last) = bytes.last_mut() {
                    *last ^= 0xFF;
                }
                Ok(bytes)
            }
            _ => unreachable!("error-shaped faults handled above"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysOk;
    impl FrameResponder for AlwaysOk {
        fn respond(&mut self, _request: &[u8]) -> Result<Vec<u8>, TransportError> {
            // A minimal valid frame: `$ M > 0 0 <checksum=0>`.
            Ok(vec![b'$', b'M', b'>', 0, 0, 0])
        }
    }

    #[test]
    fn an_error_fault_fires_once_at_the_chosen_ordinal() {
        let mut injector = FaultInjector::new(AlwaysOk, 1, Fault::Timeout);
        assert!(injector.respond(b"a").is_ok()); // call 0
        assert_eq!(injector.respond(b"b"), Err(TransportError::Timeout)); // call 1
        assert!(injector.respond(b"c").is_ok()); // call 2 delegates again
    }

    #[test]
    fn a_corrupt_frame_fault_returns_undecodable_bytes() {
        let mut injector = FaultInjector::new(AlwaysOk, 0, Fault::CorruptFrame);
        let bytes = injector.respond(b"x").unwrap();
        assert_ne!(&bytes[..2], b"$M");
    }

    #[test]
    fn a_bad_checksum_fault_flips_the_last_byte() {
        let mut injector = FaultInjector::new(AlwaysOk, 0, Fault::BadChecksum);
        let bytes = injector.respond(b"x").unwrap();
        // The mock's checksum byte was 0x00; flipping it yields 0xFF.
        assert_eq!(bytes[bytes.len() - 1], 0xFF);
    }

    #[test]
    fn a_scheduled_injector_fires_at_each_chosen_ordinal_only() {
        let mut injector = ScheduledFaultInjector::new(
            AlwaysOk,
            vec![(1, Fault::Timeout), (3, Fault::Disconnected)],
        );
        assert!(injector.respond(b"a").is_ok()); // 0
        assert_eq!(injector.respond(b"b"), Err(TransportError::Timeout)); // 1
        assert!(injector.respond(b"c").is_ok()); // 2
        assert_eq!(injector.respond(b"d"), Err(TransportError::Disconnected)); // 3
        assert!(injector.respond(b"e").is_ok()); // 4
        assert_eq!(injector.calls(), 5);
    }
}
