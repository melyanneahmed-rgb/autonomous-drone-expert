#![forbid(unsafe_code)]

//! # `ade-transport` — logical transport contracts, audit and simulations (M1)
//!
//! There is **no production adapter** here in M1 and no OS-specific code. The crate defines
//! the logical transport contract and provides two simulations: an in-memory
//! [`MockTransport`] driven by a [`FrameResponder`], and a deterministic [`ReplayTransport`]
//! driven by a project-owned transcript.
//!
//! Two safety properties are structural:
//! * every outbound frame passes through an [`AuditSink`] recording metadata only — never
//!   the payload bytes;
//! * while a session is [`Phase::Identifying`], any write command is refused
//!   ([`TransportError::WriteDuringIdentify`]).

use ade_protocol_msp::{CommandId, Direction, MspError, decode_frame};

/// Errors surfaced by a logical transport. Structural only — never raw payload content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The port/device is busy (another client holds it).
    PortBusy,
    /// Permission to access the device was denied.
    PermissionDenied,
    /// No driver is available for the device.
    MissingDriver,
    /// The device disconnected mid-exchange.
    Disconnected,
    /// A read timed out.
    Timeout,
    /// No reply was produced for a request.
    NoReply,
    /// A frame could not be decoded.
    Malformed(MspError),
    /// A write command was attempted during identification.
    WriteDuringIdentify(u8),
    /// The transport was asked to send a frame the transcript did not expect.
    UnexpectedFrame,
    /// The transcript expected a different frame at this position (replies out of order).
    OrderMismatch,
    /// The transcript has no more steps.
    ReplayExhausted,
    /// The transport is not open.
    NotOpen,
}

/// The phase of a session, used to enforce zero writes during identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Identity is being read; writes are forbidden.
    Identifying,
    /// Normal operation; writes are permitted (subject to the safety gate elsewhere).
    Operational,
}

/// Whether a command mutates the device or requests a reboot (i.e. is not a pure read).
#[must_use]
pub fn is_write_command(command: CommandId) -> bool {
    matches!(
        command,
        CommandId::SetBeeperConfig | CommandId::EepromWrite | CommandId::Reboot
    )
}

/// One audited outbound frame: metadata only, never the payload content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// Frame direction.
    pub direction: Direction,
    /// The raw command byte.
    pub command: u8,
    /// Payload length (a count only).
    pub payload_len: usize,
    /// The frame checksum byte.
    pub checksum: u8,
}

/// A sink that records outbound frame metadata for the case log.
pub trait AuditSink {
    /// Record one outbound frame's metadata.
    fn record(&mut self, entry: AuditEntry);
}

/// An in-memory audit sink.
#[derive(Debug, Default, Clone)]
pub struct InMemoryAudit {
    entries: Vec<AuditEntry>,
}

impl InMemoryAudit {
    /// A new, empty audit log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded entries.
    #[must_use]
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }
}

impl AuditSink for InMemoryAudit {
    fn record(&mut self, entry: AuditEntry) {
        self.entries.push(entry);
    }
}

/// Derive an [`AuditEntry`] from a raw outbound frame (metadata only).
///
/// # Errors
/// [`TransportError::Malformed`] if the frame does not decode.
pub fn audit_entry_for(frame: &[u8]) -> Result<AuditEntry, TransportError> {
    let decoded = decode_frame(frame).map_err(TransportError::Malformed)?;
    Ok(AuditEntry {
        direction: decoded.direction,
        command: decoded.command,
        payload_len: decoded.payload_len(),
        checksum: *frame
            .last()
            .expect("a decoded frame has at least the checksum byte"),
    })
}

/// Something that answers an MSP request frame with a reply frame. Implemented by the mock
/// flight controller and by fault injectors.
pub trait FrameResponder {
    /// Answer `request` with a reply frame, or a transport error (e.g. an injected fault).
    ///
    /// # Errors
    /// Any [`TransportError`] the responder wishes to surface.
    fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError>;
}

/// The logical transport contract. Mock and Replay implement it; there is no OS adapter.
pub trait LogicalTransport {
    /// Open the logical connection.
    ///
    /// # Errors
    /// A [`TransportError`] if the device cannot be opened (busy, permission, driver).
    fn open(&mut self) -> Result<(), TransportError>;

    /// Send one request frame and return the reply frame (write-then-read).
    ///
    /// # Errors
    /// A [`TransportError`] on any exchange failure.
    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError>;

    /// Close the logical connection.
    fn close(&mut self);
}

/// An in-memory transport that forwards each request to a [`FrameResponder`], recording
/// every outbound frame in an [`AuditSink`], and refusing writes during identification.
#[derive(Debug)]
pub struct MockTransport<R: FrameResponder, A: AuditSink> {
    responder: R,
    audit: A,
    phase: Phase,
    open: bool,
}

impl<R: FrameResponder, A: AuditSink> MockTransport<R, A> {
    /// Build a mock transport in the identifying phase.
    pub fn new(responder: R, audit: A) -> Self {
        Self {
            responder,
            audit,
            phase: Phase::Identifying,
            open: false,
        }
    }

    /// Move the session to the operational phase (identification finished).
    pub fn enter_operational(&mut self) {
        self.phase = Phase::Operational;
    }

    /// The audit sink.
    pub fn audit(&self) -> &A {
        &self.audit
    }

    fn guard_and_audit(&mut self, request: &[u8]) -> Result<(), TransportError> {
        let entry = audit_entry_for(request)?;
        if matches!(self.phase, Phase::Identifying) {
            if let Some(command) = CommandId::from_u8(entry.command) {
                if is_write_command(command) {
                    return Err(TransportError::WriteDuringIdentify(entry.command));
                }
            }
        }
        self.audit.record(entry);
        Ok(())
    }
}

impl<R: FrameResponder, A: AuditSink> LogicalTransport for MockTransport<R, A> {
    fn open(&mut self) -> Result<(), TransportError> {
        self.open = true;
        Ok(())
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        if !self.open {
            return Err(TransportError::NotOpen);
        }
        self.guard_and_audit(request)?;
        self.responder.respond(request)
    }

    fn close(&mut self) {
        self.open = false;
    }
}

/// One step of a replay transcript: the exact frame expected out, and what comes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayStep {
    /// The exact outbound request frame expected at this position.
    pub expected_request: Vec<u8>,
    /// What the transcript returns: a reply frame, or an injected fault.
    pub response: ReplayResponse,
}

/// What a replay step returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayResponse {
    /// Return this reply frame.
    Reply(Vec<u8>),
    /// Inject this transport error instead of a reply.
    Injected(TransportError),
}

/// A description of how a replay diverged from its transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// The transcript index at which the divergence occurred.
    pub step: usize,
    /// The kind of divergence.
    pub kind: TransportError,
}

/// A deterministic transport that plays a project-owned transcript. It rejects any outbound
/// frame the transcript did not expect, reports order mismatches and exhaustion, and can
/// inject faults — all without ever storing raw payload content beyond the expected frames
/// it was constructed with.
#[derive(Debug)]
pub struct ReplayTransport<A: AuditSink> {
    steps: Vec<ReplayStep>,
    cursor: usize,
    audit: A,
    phase: Phase,
    open: bool,
    divergence: Option<Divergence>,
}

impl<A: AuditSink> ReplayTransport<A> {
    /// Build a replay transport from a transcript.
    pub fn new(steps: Vec<ReplayStep>, audit: A) -> Self {
        Self {
            steps,
            cursor: 0,
            audit,
            phase: Phase::Identifying,
            open: false,
            divergence: None,
        }
    }

    /// Move to the operational phase.
    pub fn enter_operational(&mut self) {
        self.phase = Phase::Operational;
    }

    /// The audit sink.
    pub fn audit(&self) -> &A {
        &self.audit
    }

    /// The first divergence observed, if any.
    #[must_use]
    pub fn divergence(&self) -> Option<&Divergence> {
        self.divergence.as_ref()
    }

    /// Whether every transcript step was consumed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.cursor == self.steps.len()
    }

    fn record_divergence(&mut self, kind: TransportError) -> TransportError {
        if self.divergence.is_none() {
            self.divergence = Some(Divergence {
                step: self.cursor,
                kind: kind.clone(),
            });
        }
        kind
    }
}

impl<A: AuditSink> LogicalTransport for ReplayTransport<A> {
    fn open(&mut self) -> Result<(), TransportError> {
        self.open = true;
        Ok(())
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        if !self.open {
            return Err(TransportError::NotOpen);
        }
        let entry = audit_entry_for(request)?;
        if matches!(self.phase, Phase::Identifying) {
            if let Some(command) = CommandId::from_u8(entry.command) {
                if is_write_command(command) {
                    return Err(
                        self.record_divergence(TransportError::WriteDuringIdentify(entry.command))
                    );
                }
            }
        }
        if self.cursor >= self.steps.len() {
            return Err(self.record_divergence(TransportError::ReplayExhausted));
        }
        if self.steps[self.cursor].expected_request != request {
            // Distinguish "wrong frame entirely" from "a later-expected frame arrived early".
            let appears_later = self.steps[self.cursor + 1..]
                .iter()
                .any(|step| step.expected_request == request);
            let kind = if appears_later {
                TransportError::OrderMismatch
            } else {
                TransportError::UnexpectedFrame
            };
            return Err(self.record_divergence(kind));
        }
        self.audit.record(entry);
        let response = self.steps[self.cursor].response.clone();
        self.cursor += 1;
        match response {
            ReplayResponse::Reply(bytes) => Ok(bytes),
            ReplayResponse::Injected(error) => Err(error),
        }
    }

    fn close(&mut self) {
        self.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_protocol_msp::{CommandId, SetBeeperConfig, encode_frame};

    struct EchoOk;
    impl FrameResponder for EchoOk {
        fn respond(&mut self, _request: &[u8]) -> Result<Vec<u8>, TransportError> {
            Ok(encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 46]).unwrap())
        }
    }

    fn read_frame() -> Vec<u8> {
        encode_frame(Direction::Request, CommandId::ApiVersion, &[]).unwrap()
    }

    fn write_frame() -> Vec<u8> {
        SetBeeperConfig::new(0x0001_0000).encode_request().unwrap()
    }

    #[test]
    fn mock_records_audit_metadata_only() {
        let mut t = MockTransport::new(EchoOk, InMemoryAudit::new());
        t.open().unwrap();
        t.exchange(&read_frame()).unwrap();
        let entries = t.audit().entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].command, CommandId::ApiVersion.as_u8());
        assert_eq!(entries[0].payload_len, 0);
    }

    #[test]
    fn writes_are_refused_during_identification() {
        let mut t = MockTransport::new(EchoOk, InMemoryAudit::new());
        t.open().unwrap();
        assert_eq!(
            t.exchange(&write_frame()),
            Err(TransportError::WriteDuringIdentify(
                CommandId::SetBeeperConfig.as_u8()
            )),
        );
        // The refused write is not recorded in the audit log.
        assert!(t.audit().entries().is_empty());
        t.enter_operational();
        assert!(t.exchange(&write_frame()).is_ok());
    }

    #[test]
    fn replay_rejects_an_unexpected_frame() {
        let expected = read_frame();
        let steps = vec![ReplayStep {
            expected_request: expected.clone(),
            response: ReplayResponse::Reply(vec![]),
        }];
        let mut t = ReplayTransport::new(steps, InMemoryAudit::new());
        t.open().unwrap();
        let wrong = encode_frame(Direction::Request, CommandId::FcVersion, &[]).unwrap();
        assert_eq!(t.exchange(&wrong), Err(TransportError::UnexpectedFrame));
        assert_eq!(
            t.divergence().unwrap().kind,
            TransportError::UnexpectedFrame
        );
    }

    #[test]
    fn replay_detects_order_mismatch() {
        let first = encode_frame(Direction::Request, CommandId::ApiVersion, &[]).unwrap();
        let second = encode_frame(Direction::Request, CommandId::FcVersion, &[]).unwrap();
        let steps = vec![
            ReplayStep {
                expected_request: first,
                response: ReplayResponse::Reply(vec![]),
            },
            ReplayStep {
                expected_request: second.clone(),
                response: ReplayResponse::Reply(vec![]),
            },
        ];
        let mut t = ReplayTransport::new(steps, InMemoryAudit::new());
        t.open().unwrap();
        // Send the second frame first.
        assert_eq!(t.exchange(&second), Err(TransportError::OrderMismatch));
    }

    #[test]
    fn replay_can_inject_a_fault() {
        let steps = vec![ReplayStep {
            expected_request: read_frame(),
            response: ReplayResponse::Injected(TransportError::Timeout),
        }];
        let mut t = ReplayTransport::new(steps, InMemoryAudit::new());
        t.open().unwrap();
        assert_eq!(t.exchange(&read_frame()), Err(TransportError::Timeout));
        assert!(t.is_complete());
    }
}
