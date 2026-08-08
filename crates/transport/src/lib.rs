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
//! * identification is **fail-closed**: while a session is [`Phase::Identifying`] only the
//!   four identification reads (`MSP_API_VERSION`, `MSP_FC_VARIANT`, `MSP_FC_VERSION`,
//!   `MSP_BOARD_INFO`) may reach the responder or transcript. A known write command is
//!   refused with [`TransportError::WriteDuringIdentify`]; any other command — including a
//!   snapshot read and any unknown command byte — is refused with
//!   [`TransportError::CommandNotAllowedDuringIdentify`]. Every refusal is audited as
//!   [`AuditDisposition::BlockedNotSent`], metadata only.

use ade_protocol_msp::{CommandId, Direction, MspError, decode_frame};
use ade_runtime_ports::{
    BoundaryError, IoCoordinator, IoEffect, IoResponse, OutboundPacket, TransportEffect,
    TransportFailure, TransportResult,
};
use ade_safety::{ExecutionTarget, RecoveryClass, WriteApproval, WriteCommandClass};
use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

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
    /// The caller cancelled the operation before it reached the responder/transcript.
    Cancelled,
    /// The injected monotonic deadline elapsed before the operation.
    DeadlineExceeded,
    /// No reply was produced for a request.
    NoReply,
    /// A frame could not be decoded.
    Malformed(MspError),
    /// A write command was attempted during identification.
    WriteDuringIdentify(u8),
    /// A non-write command outside the identification allow-list (or an unknown command
    /// byte) was attempted during identification. Identify is fail-closed: only the four
    /// identification reads may reach the responder or transcript.
    CommandNotAllowedDuringIdentify(u8),
    /// The transport was asked to send a frame the transcript did not expect.
    UnexpectedFrame,
    /// The transcript expected a different frame at this position (replies out of order).
    OrderMismatch,
    /// The transcript has no more steps.
    ReplayExhausted,
    /// The transport is not open.
    NotOpen,
    /// An operational write/reboot was attempted without typed approval evidence.
    WriteApprovalRequired(u8),
    /// A proven read attempted to borrow write approval evidence.
    ReadBorrowedWriteApproval(u8),
    /// The actual command class did not match the approval.
    ApprovalClassMismatch {
        actual: WriteCommandClass,
        approved: WriteCommandClass,
    },
    /// The approval's recovery class did not match the lifecycle declaration.
    ApprovalRecoveryMismatch,
    /// The approval was issued for a different execution target.
    ApprovalTargetMismatch,
    /// A command without a pinned typed classification was refused fail-closed.
    UnknownCommand(u8),
    /// The deterministic host-effect boundary refused or mismatched an operation.
    EffectBoundary(BoundaryError),
    /// A host adapter returned a stable failure without a more specific logical mapping.
    HostFailure(TransportFailure),
}

/// The phase of a session, used to enforce the fail-closed identification allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Identity is being read; only the four identification reads are permitted.
    Identifying,
    /// Normal operation; writes are permitted (subject to the safety gate elsewhere).
    Operational,
}

/// Injected monotonic time source used by deterministic simulation transports.
///
/// The transport never sleeps and never reads wall-clock time. Tests can move a
/// [`ManualClock`] explicitly and obtain identical Mock/Replay decisions.
pub trait Clock: fmt::Debug {
    /// Current monotonic time in milliseconds.
    fn now_ms(&self) -> u64;
}

/// Injected cancellation state used by deterministic simulation transports.
pub trait CancellationToken: fmt::Debug {
    /// Whether the current operation must stop before sending.
    fn is_cancelled(&self) -> bool;
}

/// Cloneable manual clock for simulations and tests.
#[derive(Debug, Default, Clone)]
pub struct ManualClock {
    now_ms: Rc<Cell<u64>>,
}

impl ManualClock {
    /// Create a clock at a caller-supplied monotonic instant.
    #[must_use]
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: Rc::new(Cell::new(now_ms)),
        }
    }

    /// Move the clock forward without sleeping.
    pub fn advance(&self, delta_ms: u64) {
        self.now_ms.set(self.now_ms.get().saturating_add(delta_ms));
    }

    /// Set an exact monotonic instant.
    pub fn set(&self, now_ms: u64) {
        self.now_ms.set(now_ms);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.get()
    }
}

/// Cloneable cancellation flag for simulations and tests.
#[derive(Debug, Default, Clone)]
pub struct CancellationFlag {
    cancelled: Rc<Cell<bool>>,
}

impl CancellationFlag {
    /// Mark subsequent operations cancelled.
    pub fn cancel(&self) {
        self.cancelled.set(true);
    }

    /// Clear cancellation for a new deterministic test step.
    pub fn reset(&self) {
        self.cancelled.set(false);
    }
}

impl CancellationToken for CancellationFlag {
    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }
}

#[derive(Debug)]
struct TransportControl {
    clock: Box<dyn Clock>,
    cancellation: Box<dyn CancellationToken>,
    deadline_ms: Option<u64>,
}

impl TransportControl {
    fn unbounded() -> Self {
        Self {
            clock: Box::new(ManualClock::default()),
            cancellation: Box::new(CancellationFlag::default()),
            deadline_ms: None,
        }
    }

    fn injected(
        clock: impl Clock + 'static,
        cancellation: impl CancellationToken + 'static,
        deadline_ms: Option<u64>,
    ) -> Self {
        Self {
            clock: Box::new(clock),
            cancellation: Box::new(cancellation),
            deadline_ms,
        }
    }

    fn check(&self) -> Result<(), TransportError> {
        if self.cancellation.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        if self
            .deadline_ms
            .is_some_and(|deadline| self.clock.now_ms() >= deadline)
        {
            return Err(TransportError::DeadlineExceeded);
        }
        Ok(())
    }
}

/// Whether a command mutates the device or requests a reboot (i.e. is not a pure read).
#[must_use]
pub fn is_write_command(command: CommandId) -> bool {
    matches!(
        command,
        CommandId::SetBeeperConfig | CommandId::EepromWrite | CommandId::Reboot
    )
}

/// The actual write class fixed by a pinned command id.
#[must_use]
pub const fn command_class(command: CommandId) -> WriteCommandClass {
    match command {
        CommandId::SetBeeperConfig => WriteCommandClass::TransientConfig,
        CommandId::EepromWrite => WriteCommandClass::PersistentConfig,
        CommandId::Reboot => WriteCommandClass::Reboot,
        CommandId::ApiVersion
        | CommandId::FcVariant
        | CommandId::FcVersion
        | CommandId::BoardInfo
        | CommandId::BeeperConfig => WriteCommandClass::NoWrite,
    }
}

fn authority_refusal(
    command: u8,
    expected_target: ExecutionTarget,
    approval: Option<(&WriteApproval, RecoveryClass)>,
) -> Option<TransportError> {
    let Some(command_id) = CommandId::from_u8(command) else {
        return Some(TransportError::UnknownCommand(command));
    };
    let actual = command_class(command_id);
    match (actual, approval) {
        (WriteCommandClass::NoWrite, Some(_)) => {
            Some(TransportError::ReadBorrowedWriteApproval(command))
        }
        (WriteCommandClass::NoWrite, None) => None,
        (_, None) => Some(TransportError::WriteApprovalRequired(command)),
        (_, Some((approval, _declared_recovery))) if approval.target() != expected_target => {
            Some(TransportError::ApprovalTargetMismatch)
        }
        (_, Some((approval, _declared_recovery))) if approval.class() != actual => {
            Some(TransportError::ApprovalClassMismatch {
                actual,
                approved: approval.class(),
            })
        }
        (_, Some((approval, declared_recovery))) if approval.recovery() != declared_recovery => {
            Some(TransportError::ApprovalRecoveryMismatch)
        }
        (_, Some(_)) => None,
    }
}

/// Whether a command is one of the four identification reads permitted while identifying.
#[must_use]
pub fn is_identify_command(command: CommandId) -> bool {
    matches!(
        command,
        CommandId::ApiVersion | CommandId::FcVariant | CommandId::FcVersion | CommandId::BoardInfo
    )
}

/// The fail-closed identification guard: why a command byte must be refused during
/// [`Phase::Identifying`], or `None` if it is one of the four identification reads.
///
/// Known write commands keep their dedicated [`TransportError::WriteDuringIdentify`]; every
/// other command — including a snapshot read like `MSP_BEEPER_CONFIG` and any unknown
/// command byte — is refused with [`TransportError::CommandNotAllowedDuringIdentify`].
fn identify_refusal(command: u8) -> Option<TransportError> {
    match CommandId::from_u8(command) {
        Some(known) if is_identify_command(known) => None,
        Some(known) if is_write_command(known) => {
            Some(TransportError::WriteDuringIdentify(command))
        }
        _ => Some(TransportError::CommandNotAllowedDuringIdentify(command)),
    }
}

/// Whether an audited frame was actually sent to the responder or blocked before sending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditDisposition {
    /// The frame passed the guard and was handed to the responder.
    Sent,
    /// The frame was refused by a guard (e.g. a write during identification) and never sent.
    /// Its metadata is still recorded so the log distinguishes an attempt from a real send.
    BlockedNotSent,
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
    /// Whether the frame was sent or blocked before sending.
    pub disposition: AuditDisposition,
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

/// Derive an [`AuditEntry`] from a raw outbound frame (metadata only). The entry defaults to
/// [`AuditDisposition::Sent`]; a guard that refuses the frame overrides it to
/// [`AuditDisposition::BlockedNotSent`] before recording.
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
        disposition: AuditDisposition::Sent,
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

/// Phase control for a simulation transport: the orchestrator moves the transport between
/// the fail-closed identification phase and normal operation. Re-entering identification is
/// required after every reboot/reconnect so the allow-list applies to re-identification too.
pub trait PhasedTransport {
    /// Enter the operational phase (identification finished).
    fn enter_operational(&mut self);
    /// Re-enter the fail-closed identification phase (e.g. after a reconnect).
    fn begin_identification(&mut self);
}

/// Read access to the metadata-only audit trail of a simulation transport.
pub trait AuditAccess {
    /// The recorded outbound-frame metadata, in order.
    fn audit_entries(&self) -> &[AuditEntry];
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

    /// Send one write/reboot request while retaining the original typed approval and the
    /// lifecycle's declared recovery class through the transport boundary.
    ///
    /// # Errors
    /// A [`TransportError`] if the actual frame command, approval, recovery, or exchange
    /// fails. Implementations must refuse before forwarding bytes.
    fn exchange_with_approval(
        &mut self,
        request: &[u8],
        approval: &WriteApproval,
        declared_recovery: RecoveryClass,
    ) -> Result<Vec<u8>, TransportError>;

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
    control: TransportControl,
}

impl<R: FrameResponder, A: AuditSink> MockTransport<R, A> {
    /// Build a mock transport in the identifying phase.
    pub fn new(responder: R, audit: A) -> Self {
        Self {
            responder,
            audit,
            phase: Phase::Identifying,
            open: false,
            control: TransportControl::unbounded(),
        }
    }

    /// Build a mock transport with injected time, cancellation and an optional absolute
    /// monotonic deadline.
    pub fn new_with_control(
        responder: R,
        audit: A,
        clock: impl Clock + 'static,
        cancellation: impl CancellationToken + 'static,
        deadline_ms: Option<u64>,
    ) -> Self {
        Self {
            responder,
            audit,
            phase: Phase::Identifying,
            open: false,
            control: TransportControl::injected(clock, cancellation, deadline_ms),
        }
    }

    /// Move the session to the operational phase (identification finished).
    pub fn enter_operational(&mut self) {
        self.phase = Phase::Operational;
    }

    /// Re-enter the fail-closed identification phase (used after a reconnect).
    pub fn begin_identification(&mut self) {
        self.phase = Phase::Identifying;
    }

    /// The audit sink.
    pub fn audit(&self) -> &A {
        &self.audit
    }

    fn guard_and_audit(
        &mut self,
        request: &[u8],
        approval: Option<(&WriteApproval, RecoveryClass)>,
    ) -> Result<(), TransportError> {
        let mut entry = audit_entry_for(request)?;
        if matches!(self.phase, Phase::Identifying) {
            if let Some(error) = identify_refusal(entry.command) {
                // Record the blocked attempt as metadata only — the payload is never
                // logged and the responder is never called — then refuse the frame.
                entry.disposition = AuditDisposition::BlockedNotSent;
                self.audit.record(entry);
                return Err(error);
            }
        }
        if let Some(error) = authority_refusal(entry.command, ExecutionTarget::Mock, approval) {
            entry.disposition = AuditDisposition::BlockedNotSent;
            self.audit.record(entry);
            return Err(error);
        }
        self.audit.record(entry);
        Ok(())
    }
}

impl<R: FrameResponder, A: AuditSink> LogicalTransport for MockTransport<R, A> {
    fn open(&mut self) -> Result<(), TransportError> {
        self.control.check()?;
        self.open = true;
        Ok(())
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        if !self.open {
            return Err(TransportError::NotOpen);
        }
        self.control.check()?;
        self.guard_and_audit(request, None)?;
        self.responder.respond(request)
    }

    fn exchange_with_approval(
        &mut self,
        request: &[u8],
        approval: &WriteApproval,
        declared_recovery: RecoveryClass,
    ) -> Result<Vec<u8>, TransportError> {
        if !self.open {
            return Err(TransportError::NotOpen);
        }
        self.control.check()?;
        self.guard_and_audit(request, Some((approval, declared_recovery)))?;
        self.responder.respond(request)
    }

    fn close(&mut self) {
        self.open = false;
    }
}

impl<R: FrameResponder, A: AuditSink> PhasedTransport for MockTransport<R, A> {
    fn enter_operational(&mut self) {
        MockTransport::enter_operational(self);
    }

    fn begin_identification(&mut self) {
        MockTransport::begin_identification(self);
    }
}

impl<R: FrameResponder> AuditAccess for MockTransport<R, InMemoryAudit> {
    fn audit_entries(&self) -> &[AuditEntry] {
        self.audit.entries()
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
    control: TransportControl,
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
            control: TransportControl::unbounded(),
        }
    }

    /// Build a replay transport with injected time, cancellation and an optional absolute
    /// monotonic deadline.
    pub fn new_with_control(
        steps: Vec<ReplayStep>,
        audit: A,
        clock: impl Clock + 'static,
        cancellation: impl CancellationToken + 'static,
        deadline_ms: Option<u64>,
    ) -> Self {
        Self {
            steps,
            cursor: 0,
            audit,
            phase: Phase::Identifying,
            open: false,
            divergence: None,
            control: TransportControl::injected(clock, cancellation, deadline_ms),
        }
    }

    /// Move to the operational phase.
    pub fn enter_operational(&mut self) {
        self.phase = Phase::Operational;
    }

    /// Re-enter the fail-closed identification phase (used after a reconnect).
    pub fn begin_identification(&mut self) {
        self.phase = Phase::Identifying;
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
        self.control.check()?;
        self.open = true;
        Ok(())
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        if !self.open {
            return Err(TransportError::NotOpen);
        }
        self.control.check()?;
        let entry = audit_entry_for(request)?;
        if matches!(self.phase, Phase::Identifying) {
            if let Some(error) = identify_refusal(entry.command) {
                // Record the blocked attempt (metadata only) before refusing, so the audit
                // log shows the attempt just as the mock transport does. The transcript
                // cursor is deliberately NOT advanced: the blocked frame never reached it.
                let mut blocked = entry.clone();
                blocked.disposition = AuditDisposition::BlockedNotSent;
                self.audit.record(blocked);
                return Err(self.record_divergence(error));
            }
        }
        if let Some(error) = authority_refusal(entry.command, ExecutionTarget::Replay, None) {
            let mut blocked = entry;
            blocked.disposition = AuditDisposition::BlockedNotSent;
            self.audit.record(blocked);
            return Err(self.record_divergence(error));
        }
        self.exchange_after_guard(request, entry)
    }

    fn exchange_with_approval(
        &mut self,
        request: &[u8],
        approval: &WriteApproval,
        declared_recovery: RecoveryClass,
    ) -> Result<Vec<u8>, TransportError> {
        if !self.open {
            return Err(TransportError::NotOpen);
        }
        self.control.check()?;
        let entry = audit_entry_for(request)?;
        if matches!(self.phase, Phase::Identifying) {
            if let Some(error) = identify_refusal(entry.command) {
                let mut blocked = entry;
                blocked.disposition = AuditDisposition::BlockedNotSent;
                self.audit.record(blocked);
                return Err(self.record_divergence(error));
            }
        }
        if let Some(error) = authority_refusal(
            entry.command,
            ExecutionTarget::Replay,
            Some((approval, declared_recovery)),
        ) {
            let mut blocked = entry;
            blocked.disposition = AuditDisposition::BlockedNotSent;
            self.audit.record(blocked);
            return Err(self.record_divergence(error));
        }
        self.exchange_after_guard(request, entry)
    }

    fn close(&mut self) {
        self.open = false;
    }
}

impl<A: AuditSink> ReplayTransport<A> {
    fn exchange_after_guard(
        &mut self,
        request: &[u8],
        entry: AuditEntry,
    ) -> Result<Vec<u8>, TransportError> {
        if self.cursor >= self.steps.len() {
            self.audit.record(entry);
            return Err(self.record_divergence(TransportError::ReplayExhausted));
        }
        if self.steps[self.cursor].expected_request != request {
            self.audit.record(entry);
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
}

impl<A: AuditSink> PhasedTransport for ReplayTransport<A> {
    fn enter_operational(&mut self) {
        ReplayTransport::enter_operational(self);
    }

    fn begin_identification(&mut self) {
        ReplayTransport::begin_identification(self);
    }
}

impl AuditAccess for ReplayTransport<InMemoryAudit> {
    fn audit_entries(&self) -> &[AuditEntry] {
        self.audit.entries()
    }
}

/// A synchronous test/native bridge for host-owned transport effects.
///
/// The production browser boundary remains asynchronous. This trait lets the established
/// synchronous M1 lifecycle exercise exactly the same typed effects without introducing a
/// second executor or lifecycle. Host code receives an [`IoEffect`] only after packet
/// authority has been validated.
pub trait TransportEffectHost {
    /// Execute one already-authorised transport effect and return its typed response.
    ///
    /// A logical transport error is returned outside [`IoResponse`] so Mock/Replay fault
    /// classifications remain lossless in parity tests.
    fn execute(&mut self, effect: IoEffect) -> Result<IoResponse, TransportError>;
    /// Propagate the lifecycle phase to the host transport.
    fn enter_operational(&mut self);
    /// Re-arm the identification allow-list on the host transport.
    fn begin_identification(&mut self);
    /// Expose only the existing metadata audit.
    fn audit_entries(&self) -> &[AuditEntry];
}

/// Adapts an established logical Mock/Replay transport into a host-effect responder.
#[derive(Debug)]
pub struct LogicalEffectHost<T> {
    target: ExecutionTarget,
    inner: T,
}

impl<T> LogicalEffectHost<T> {
    #[must_use]
    pub const fn new(target: ExecutionTarget, inner: T) -> Self {
        Self { target, inner }
    }

    #[must_use]
    pub const fn inner(&self) -> &T {
        &self.inner
    }

    #[must_use]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: LogicalTransport + PhasedTransport + AuditAccess> TransportEffectHost
    for LogicalEffectHost<T>
{
    fn execute(&mut self, effect: IoEffect) -> Result<IoResponse, TransportError> {
        let IoEffect::Transport { request_id, effect } = effect else {
            return Err(TransportError::EffectBoundary(
                BoundaryError::ResponseKindMismatch,
            ));
        };
        let result = match effect {
            TransportEffect::OpenSelectedReadOnlyPort => {
                self.inner.open()?;
                TransportResult::Open(Ok(()))
            }
            TransportEffect::Exchange(packet) => {
                let reply = match packet.approval() {
                    Some(approval) => {
                        if approval.target() != self.target {
                            return Err(TransportError::ApprovalTargetMismatch);
                        }
                        self.inner.exchange_with_approval(
                            packet.bytes(),
                            approval,
                            packet
                                .approved_recovery()
                                .expect("approved packet retains recovery evidence"),
                        )?
                    }
                    None => self.inner.exchange(packet.bytes())?,
                };
                TransportResult::Exchange(Ok(reply))
            }
            TransportEffect::Close => {
                self.inner.close();
                TransportResult::Close(Ok(()))
            }
        };
        Ok(IoResponse::Transport { request_id, result })
    }

    fn enter_operational(&mut self) {
        self.inner.enter_operational();
    }

    fn begin_identification(&mut self) {
        self.inner.begin_identification();
    }

    fn audit_entries(&self) -> &[AuditEntry] {
        self.inner.audit_entries()
    }
}

/// The current M1 [`LogicalTransport`] driven through typed host effects.
///
/// This is an adapter around the existing executor/lifecycle, not an alternate execution
/// path. The host cannot observe command bytes until the actual frame command has been
/// classified and all target/class/recovery approval evidence has matched.
#[derive(Debug)]
pub struct EffectTransport<H> {
    target: ExecutionTarget,
    coordinator: IoCoordinator,
    host: H,
}

impl<H> EffectTransport<H> {
    #[must_use]
    pub fn new(target: ExecutionTarget, host: H) -> Self {
        Self {
            target,
            coordinator: IoCoordinator::new(),
            host,
        }
    }

    #[must_use]
    pub const fn target(&self) -> ExecutionTarget {
        self.target
    }

    #[must_use]
    pub const fn host(&self) -> &H {
        &self.host
    }

    #[must_use]
    pub fn into_host(self) -> H {
        self.host
    }
}

impl<H: TransportEffectHost> EffectTransport<H> {
    fn round_trip(&mut self, effect: IoEffect) -> Result<IoResponse, TransportError> {
        let request_id = effect.request_id();
        match self.host.execute(effect) {
            Ok(response) => self
                .coordinator
                .accept(response)
                .map_err(TransportError::EffectBoundary),
            Err(error) => {
                self.coordinator
                    .cancel_transport(request_id)
                    .map_err(TransportError::EffectBoundary)?;
                Err(error)
            }
        }
    }

    fn exchange_packet(&mut self, packet: OutboundPacket) -> Result<Vec<u8>, TransportError> {
        let effect = self
            .coordinator
            .begin_transport(TransportEffect::Exchange(packet))
            .map_err(TransportError::EffectBoundary)?;
        match self.round_trip(effect)? {
            IoResponse::Transport {
                result: TransportResult::Exchange(result),
                ..
            } => result.map_err(map_host_failure),
            _ => Err(TransportError::EffectBoundary(
                BoundaryError::ResponseKindMismatch,
            )),
        }
    }
}

fn map_host_failure(failure: TransportFailure) -> TransportError {
    match failure {
        TransportFailure::PortBusy => TransportError::PortBusy,
        TransportFailure::PermissionDenied => TransportError::PermissionDenied,
        TransportFailure::MissingDriver => TransportError::MissingDriver,
        TransportFailure::Disconnected => TransportError::Disconnected,
        TransportFailure::Timeout => TransportError::Timeout,
        TransportFailure::Cancelled => TransportError::Cancelled,
        TransportFailure::Unknown => TransportError::HostFailure(failure),
    }
}

impl<H: TransportEffectHost> LogicalTransport for EffectTransport<H> {
    fn open(&mut self) -> Result<(), TransportError> {
        let effect = self
            .coordinator
            .begin_transport(TransportEffect::OpenSelectedReadOnlyPort)
            .map_err(TransportError::EffectBoundary)?;
        match self.round_trip(effect)? {
            IoResponse::Transport {
                result: TransportResult::Open(result),
                ..
            } => result.map_err(map_host_failure),
            _ => Err(TransportError::EffectBoundary(
                BoundaryError::ResponseKindMismatch,
            )),
        }
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        let packet =
            OutboundPacket::read_only(request.to_vec()).map_err(TransportError::EffectBoundary)?;
        self.exchange_packet(packet)
    }

    fn exchange_with_approval(
        &mut self,
        request: &[u8],
        approval: &WriteApproval,
        declared_recovery: RecoveryClass,
    ) -> Result<Vec<u8>, TransportError> {
        if approval.target() != self.target {
            return Err(TransportError::ApprovalTargetMismatch);
        }
        if approval.recovery() != declared_recovery {
            return Err(TransportError::ApprovalRecoveryMismatch);
        }
        let packet = OutboundPacket::approved(request.to_vec(), approval.clone())
            .map_err(TransportError::EffectBoundary)?;
        self.exchange_packet(packet)
    }

    fn close(&mut self) {
        let Ok(effect) = self.coordinator.begin_transport(TransportEffect::Close) else {
            return;
        };
        let _ = self.round_trip(effect);
    }
}

impl<H: TransportEffectHost> PhasedTransport for EffectTransport<H> {
    fn enter_operational(&mut self) {
        self.host.enter_operational();
    }

    fn begin_identification(&mut self) {
        self.host.begin_identification();
    }
}

impl<H: TransportEffectHost> AuditAccess for EffectTransport<H> {
    fn audit_entries(&self) -> &[AuditEntry] {
        self.host.audit_entries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_protocol_msp::{CommandId, SetBeeperConfig, encode_frame};
    use ade_safety::authorize_write;

    struct EchoOk;
    impl FrameResponder for EchoOk {
        fn respond(&mut self, _request: &[u8]) -> Result<Vec<u8>, TransportError> {
            Ok(encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 46]).unwrap())
        }
    }

    fn request_with_payload(command: CommandId, payload: &[u8]) -> Vec<u8> {
        encode_frame(Direction::Request, command, payload).unwrap()
    }

    fn set_request() -> Vec<u8> {
        request_with_payload(
            CommandId::SetBeeperConfig,
            &SetBeeperConfig::new(7).payload(),
        )
    }

    #[test]
    fn operational_write_without_approval_never_reaches_responder_or_replay_cursor() {
        let mut mock = MockTransport::new(CountingOk { calls: 0 }, InMemoryAudit::new());
        mock.open().unwrap();
        mock.enter_operational();
        assert_eq!(
            mock.exchange(&set_request()),
            Err(TransportError::WriteApprovalRequired(
                CommandId::SetBeeperConfig.as_u8()
            ))
        );
        assert_eq!(mock.responder.calls, 0);
        assert_eq!(
            mock.audit.entries()[0].disposition,
            AuditDisposition::BlockedNotSent
        );

        let mut replay = ReplayTransport::new(
            vec![ReplayStep {
                expected_request: set_request(),
                response: ReplayResponse::Reply(request_with_payload(
                    CommandId::SetBeeperConfig,
                    &[],
                )),
            }],
            InMemoryAudit::new(),
        );
        replay.open().unwrap();
        replay.enter_operational();
        assert!(matches!(
            replay.exchange(&set_request()),
            Err(TransportError::WriteApprovalRequired(_))
        ));
        assert_eq!(
            replay.cursor, 0,
            "blocked write must not move replay cursor"
        );
        assert_eq!(
            replay.audit.entries()[0].disposition,
            AuditDisposition::BlockedNotSent
        );
    }

    #[test]
    fn effect_transport_refuses_authority_mismatches_before_host_bytes() {
        let host = LogicalEffectHost::new(
            ExecutionTarget::Mock,
            MockTransport::new(EchoOk, InMemoryAudit::new()),
        );
        let mut effect = EffectTransport::new(ExecutionTarget::Mock, host);
        effect.open().unwrap();
        effect.enter_operational();

        assert!(matches!(
            effect.exchange(&set_request()),
            Err(TransportError::EffectBoundary(
                BoundaryError::PacketRequiresApproval { .. }
            ))
        ));

        let replay_approval = authorize_write(
            ExecutionTarget::Replay,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .unwrap();
        assert_eq!(
            effect.exchange_with_approval(
                &set_request(),
                &replay_approval,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            ),
            Err(TransportError::ApprovalTargetMismatch)
        );

        let transient = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .unwrap();
        assert_eq!(
            effect.exchange_with_approval(
                &set_request(),
                &transient,
                RecoveryClass::RestoreFromBackupSupported,
            ),
            Err(TransportError::ApprovalRecoveryMismatch)
        );

        let persistent = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::PersistentConfig,
            RecoveryClass::AutomaticRollbackSupported,
        )
        .unwrap();
        assert!(matches!(
            effect.exchange_with_approval(
                &set_request(),
                &persistent,
                RecoveryClass::AutomaticRollbackSupported,
            ),
            Err(TransportError::EffectBoundary(
                BoundaryError::PacketClassMismatch { .. }
            ))
        ));

        assert!(matches!(
            effect.exchange_with_approval(
                &request_with_payload(CommandId::ApiVersion, &[]),
                &transient,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            ),
            Err(TransportError::EffectBoundary(
                BoundaryError::PacketClassMismatch { .. }
            ))
        ));
        assert!(
            effect.audit_entries().is_empty(),
            "host transport must not see rejected command bytes"
        );
    }

    /// A responder that counts how many requests actually reached it.
    struct CountingOk {
        calls: usize,
    }
    impl FrameResponder for CountingOk {
        fn respond(&mut self, _request: &[u8]) -> Result<Vec<u8>, TransportError> {
            self.calls += 1;
            Ok(encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 46]).unwrap())
        }
    }

    #[test]
    fn direct_transports_refuse_cross_target_approval_before_responder_or_cursor() {
        let replay_approval = authorize_write(
            ExecutionTarget::Replay,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .unwrap();
        let mut mock = MockTransport::new(CountingOk { calls: 0 }, InMemoryAudit::new());
        mock.open().unwrap();
        mock.enter_operational();
        assert_eq!(
            mock.exchange_with_approval(
                &set_request(),
                &replay_approval,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            ),
            Err(TransportError::ApprovalTargetMismatch)
        );
        assert_eq!(mock.responder.calls, 0);
        assert_eq!(
            mock.audit.entries()[0].disposition,
            AuditDisposition::BlockedNotSent
        );

        let mock_approval = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .unwrap();
        let mut replay = ReplayTransport::new(
            vec![ReplayStep {
                expected_request: set_request(),
                response: ReplayResponse::Reply(request_with_payload(
                    CommandId::SetBeeperConfig,
                    &[],
                )),
            }],
            InMemoryAudit::new(),
        );
        replay.open().unwrap();
        replay.enter_operational();
        assert_eq!(
            replay.exchange_with_approval(
                &set_request(),
                &mock_approval,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            ),
            Err(TransportError::ApprovalTargetMismatch)
        );
        assert_eq!(replay.cursor, 0);
        assert_eq!(
            replay.audit.entries()[0].disposition,
            AuditDisposition::BlockedNotSent
        );
    }

    fn read_frame() -> Vec<u8> {
        encode_frame(Direction::Request, CommandId::ApiVersion, &[]).unwrap()
    }

    fn write_frame() -> Vec<u8> {
        SetBeeperConfig::new(0x0001_0000).encode_request().unwrap()
    }

    fn request(command: CommandId) -> Vec<u8> {
        encode_frame(Direction::Request, command, &[]).unwrap()
    }

    /// A checksum-valid request frame for a command byte this codec has no record of.
    /// Built by hand, test-only: production code has no API for unknown commands.
    fn unknown_command_frame() -> Vec<u8> {
        const UNKNOWN: u8 = 200;
        // `$ M < size=0 cmd checksum` with checksum = size ^ cmd = UNKNOWN.
        vec![b'$', b'M', b'<', 0, UNKNOWN, UNKNOWN]
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
        assert_eq!(entries[0].disposition, AuditDisposition::Sent);
    }

    #[test]
    fn a_write_blocked_during_identification_is_audited_as_blocked_not_sent() {
        let mut t = MockTransport::new(EchoOk, InMemoryAudit::new());
        t.open().unwrap();
        assert_eq!(
            t.exchange(&write_frame()),
            Err(TransportError::WriteDuringIdentify(
                CommandId::SetBeeperConfig.as_u8()
            )),
        );
        // The refused write IS recorded — as a blocked attempt, metadata only, never sent.
        let entries = t.audit().entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].disposition, AuditDisposition::BlockedNotSent);
        assert_eq!(entries[0].command, CommandId::SetBeeperConfig.as_u8());
        assert_eq!(entries[0].direction, Direction::Request);

        // Once operational, the same write goes through and is audited as Sent — the log now
        // distinguishes the blocked attempt from the real send.
        t.enter_operational();
        let approval = authorize_write(
            ExecutionTarget::Mock,
            WriteCommandClass::TransientConfig,
            RecoveryClass::TransientWritePendingReconcileOnResume,
        )
        .unwrap();
        assert!(
            t.exchange_with_approval(
                &write_frame(),
                &approval,
                RecoveryClass::TransientWritePendingReconcileOnResume,
            )
            .is_ok()
        );
        let entries = t.audit().entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].disposition, AuditDisposition::Sent);
        assert_eq!(entries[0].disposition, AuditDisposition::BlockedNotSent);
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

    #[test]
    fn all_four_identification_reads_pass_during_identify_on_mock() {
        let mut t = MockTransport::new(CountingOk { calls: 0 }, InMemoryAudit::new());
        t.open().unwrap();
        for command in [
            CommandId::ApiVersion,
            CommandId::FcVariant,
            CommandId::FcVersion,
            CommandId::BoardInfo,
        ] {
            assert!(
                t.exchange(&request(command)).is_ok(),
                "{command:?} must be permitted during Identify",
            );
        }
        assert_eq!(t.responder.calls, 4, "all four reads reached the responder");
        assert!(
            t.audit()
                .entries()
                .iter()
                .all(|e| e.disposition == AuditDisposition::Sent)
        );
    }

    #[test]
    fn mock_identify_blocks_a_snapshot_read_without_calling_the_responder() {
        let mut t = MockTransport::new(CountingOk { calls: 0 }, InMemoryAudit::new());
        t.open().unwrap();
        assert_eq!(
            t.exchange(&request(CommandId::BeeperConfig)),
            Err(TransportError::CommandNotAllowedDuringIdentify(
                CommandId::BeeperConfig.as_u8()
            )),
        );
        assert_eq!(
            t.responder.calls, 0,
            "the responder must never see the frame"
        );
        let entries = t.audit().entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].disposition, AuditDisposition::BlockedNotSent);
        assert_eq!(entries[0].command, CommandId::BeeperConfig.as_u8());

        // Once operational the snapshot read is permitted and reaches the responder.
        t.enter_operational();
        assert!(t.exchange(&request(CommandId::BeeperConfig)).is_ok());
        assert_eq!(t.responder.calls, 1);
        assert_eq!(
            t.audit().entries().last().unwrap().disposition,
            AuditDisposition::Sent
        );
    }

    #[test]
    fn mock_identify_blocks_an_unknown_command_byte() {
        let mut t = MockTransport::new(CountingOk { calls: 0 }, InMemoryAudit::new());
        t.open().unwrap();
        assert_eq!(
            t.exchange(&unknown_command_frame()),
            Err(TransportError::CommandNotAllowedDuringIdentify(200)),
        );
        assert_eq!(t.responder.calls, 0);
        let entries = t.audit().entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].disposition, AuditDisposition::BlockedNotSent);
        assert_eq!(entries[0].command, 200);
        assert_eq!(entries[0].payload_len, 0);
    }

    #[test]
    fn replay_identify_blocks_disallowed_commands_without_moving_the_cursor() {
        let beeper_read = request(CommandId::BeeperConfig);
        let steps = vec![ReplayStep {
            expected_request: beeper_read.clone(),
            response: ReplayResponse::Reply(vec![]),
        }];
        let mut t = ReplayTransport::new(steps, InMemoryAudit::new());
        t.open().unwrap();

        // A snapshot read during Identify is refused even though it is the transcript's
        // next expected frame — the guard fires before the transcript is consulted.
        assert_eq!(
            t.exchange(&beeper_read),
            Err(TransportError::CommandNotAllowedDuringIdentify(
                CommandId::BeeperConfig.as_u8()
            )),
        );
        // An unknown command byte is refused the same way.
        assert_eq!(
            t.exchange(&unknown_command_frame()),
            Err(TransportError::CommandNotAllowedDuringIdentify(200)),
        );
        // Both attempts were audited as blocked; the cursor never advanced.
        let entries = t.audit().entries();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .all(|e| e.disposition == AuditDisposition::BlockedNotSent)
        );
        assert!(!t.is_complete(), "the transcript cursor must not advance");
        assert_eq!(
            t.divergence().unwrap().kind,
            TransportError::CommandNotAllowedDuringIdentify(CommandId::BeeperConfig.as_u8()),
        );

        // After identification the same snapshot read is served by the transcript — proving
        // the cursor still points at the first (and only) step.
        t.enter_operational();
        assert!(t.exchange(&beeper_read).is_ok());
        assert!(t.is_complete());
        assert_eq!(
            t.audit().entries().last().unwrap().disposition,
            AuditDisposition::Sent
        );
    }

    #[test]
    fn begin_identification_re_arms_the_fail_closed_guard() {
        let mut t = MockTransport::new(CountingOk { calls: 0 }, InMemoryAudit::new());
        t.open().unwrap();
        t.enter_operational();
        assert!(t.exchange(&request(CommandId::BeeperConfig)).is_ok());
        // Re-entering identification (as after a reconnect) blocks the same read again.
        PhasedTransport::begin_identification(&mut t);
        assert_eq!(
            t.exchange(&request(CommandId::BeeperConfig)),
            Err(TransportError::CommandNotAllowedDuringIdentify(
                CommandId::BeeperConfig.as_u8()
            )),
        );
        // And the audit trail is reachable through the AuditAccess trait.
        let entries = AuditAccess::audit_entries(&t);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].disposition, AuditDisposition::BlockedNotSent);
    }

    #[test]
    fn replay_identify_permits_the_four_identification_reads() {
        let steps = [
            CommandId::ApiVersion,
            CommandId::FcVariant,
            CommandId::FcVersion,
            CommandId::BoardInfo,
        ]
        .map(|command| ReplayStep {
            expected_request: request(command),
            response: ReplayResponse::Reply(vec![]),
        })
        .to_vec();
        let mut t = ReplayTransport::new(steps, InMemoryAudit::new());
        t.open().unwrap();
        for command in [
            CommandId::ApiVersion,
            CommandId::FcVariant,
            CommandId::FcVersion,
            CommandId::BoardInfo,
        ] {
            assert!(
                t.exchange(&request(command)).is_ok(),
                "{command:?} must be permitted during Identify",
            );
        }
        assert!(t.is_complete());
        assert!(t.divergence().is_none());
    }

    #[derive(Clone)]
    struct InjectedError(TransportError);

    impl FrameResponder for InjectedError {
        fn respond(&mut self, _request: &[u8]) -> Result<Vec<u8>, TransportError> {
            Err(self.0.clone())
        }
    }

    #[test]
    fn mock_and_replay_preserve_error_and_audit_parity_for_26_failures() {
        let errors = [
            TransportError::PortBusy,
            TransportError::PermissionDenied,
            TransportError::MissingDriver,
            TransportError::Disconnected,
            TransportError::Timeout,
            TransportError::NoReply,
            TransportError::Malformed(MspError::BadPreamble),
            TransportError::WriteDuringIdentify(CommandId::SetBeeperConfig.as_u8()),
            TransportError::CommandNotAllowedDuringIdentify(CommandId::BeeperConfig.as_u8()),
            TransportError::UnexpectedFrame,
            TransportError::OrderMismatch,
            TransportError::ReplayExhausted,
            TransportError::DeadlineExceeded,
        ];
        let frames = [read_frame(), write_frame()];
        let mut cases = 0;
        for error in errors {
            for frame in &frames {
                let mut mock =
                    MockTransport::new(InjectedError(error.clone()), InMemoryAudit::new());
                let mut replay = ReplayTransport::new(
                    vec![ReplayStep {
                        expected_request: frame.clone(),
                        response: ReplayResponse::Injected(error.clone()),
                    }],
                    InMemoryAudit::new(),
                );
                mock.open().unwrap();
                replay.open().unwrap();
                mock.enter_operational();
                replay.enter_operational();

                assert_eq!(mock.exchange(frame), replay.exchange(frame), "case {cases}");
                assert_eq!(
                    mock.audit_entries(),
                    replay.audit_entries(),
                    "audit case {cases}"
                );
                cases += 1;
            }
        }
        assert_eq!(cases, 26);
    }

    #[test]
    fn injected_deadline_is_deterministic_and_never_sleeps() {
        let mock_clock = ManualClock::new(5);
        let replay_clock = ManualClock::new(5);
        let frame = read_frame();
        let mut mock = MockTransport::new_with_control(
            EchoOk,
            InMemoryAudit::new(),
            mock_clock.clone(),
            CancellationFlag::default(),
            Some(10),
        );
        let mut replay = ReplayTransport::new_with_control(
            vec![ReplayStep {
                expected_request: frame.clone(),
                response: ReplayResponse::Reply(vec![]),
            }],
            InMemoryAudit::new(),
            replay_clock.clone(),
            CancellationFlag::default(),
            Some(10),
        );
        mock.open().unwrap();
        replay.open().unwrap();
        mock.enter_operational();
        replay.enter_operational();
        mock_clock.advance(5);
        replay_clock.advance(5);
        assert_eq!(mock.exchange(&frame), Err(TransportError::DeadlineExceeded));
        assert_eq!(
            replay.exchange(&frame),
            Err(TransportError::DeadlineExceeded)
        );
        assert!(mock.audit_entries().is_empty());
        assert!(replay.audit_entries().is_empty());
    }

    #[test]
    fn injected_cancellation_prevents_open_on_both_transports() {
        let mock_cancel = CancellationFlag::default();
        let replay_cancel = CancellationFlag::default();
        mock_cancel.cancel();
        replay_cancel.cancel();
        let mut mock = MockTransport::new_with_control(
            EchoOk,
            InMemoryAudit::new(),
            ManualClock::default(),
            mock_cancel,
            None,
        );
        let mut replay = ReplayTransport::new_with_control(
            Vec::new(),
            InMemoryAudit::new(),
            ManualClock::default(),
            replay_cancel,
            None,
        );
        assert_eq!(mock.open(), Err(TransportError::Cancelled));
        assert_eq!(replay.open(), Err(TransportError::Cancelled));
    }
}
