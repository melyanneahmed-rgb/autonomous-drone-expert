#![forbid(unsafe_code)]

//! # `ade-session` — the M1 session state machine
//!
//! An explicit, total state machine for the beeper vertical slice. Every terminal path is a
//! named state; there is **no** implicit "ready" or "success" — readiness is only ever
//! [`SessionState::CompletedVerified`], reached after verification, and an unprovable outcome
//! is [`SessionState::StateUnknownRecoveryRequired`], never a silent pass.

/// The states of the M1 lifecycle. Ordering is documentation only; transitions are governed
/// by [`SessionState::may_transition_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// No transport is connected.
    Disconnected,
    /// Establishing a logical connection.
    Connecting,
    /// Reading identity; **no writes are permitted here**.
    Identifying,
    /// Reading the full configuration snapshot.
    SnapshotRead,
    /// Building the plan from the snapshot and the desired change.
    Planning,
    /// Waiting for approval to execute against a simulation target.
    AwaitingApproval,
    /// Writing the backup before any change.
    BackingUp,
    /// Applying the transient (RAM-only) write.
    ApplyingTransient,
    /// A transient write is in flight and, on resume, is reconciled — never "rolled back".
    TransientWritePendingReconcileOnResume,
    /// Committing configuration to persistent storage.
    Saving,
    /// Rebooting the device.
    Rebooting,
    /// Re-establishing the connection after a reboot.
    Reconnecting,
    /// Re-identifying and verifying the intended change and untouched fields.
    Verifying,
    /// Executing a recovery plan after a failure.
    Recovering,
    /// Terminal: verified success.
    CompletedVerified,
    /// Terminal: the previous value was restored and verified.
    CompletedRestored,
    /// Terminal: the state cannot be proven; recovery is required.
    StateUnknownRecoveryRequired,
}

impl SessionState {
    /// Whether a write command may be issued in this state. Only false during identification
    /// (and the disconnected/connecting states), enforcing "zero writes during Identify".
    #[must_use]
    pub const fn writes_permitted(self) -> bool {
        !matches!(
            self,
            SessionState::Disconnected
                | SessionState::Connecting
                | SessionState::Identifying
                | SessionState::SnapshotRead
        )
    }

    /// Whether this is a terminal state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            SessionState::CompletedVerified
                | SessionState::CompletedRestored
                | SessionState::StateUnknownRecoveryRequired
        )
    }

    /// The states reachable in one step from `self`. Any failure may drop to recovery or, if
    /// unprovable, to [`SessionState::StateUnknownRecoveryRequired`]; that is expressed by
    /// making those transitions broadly available rather than by a silent fallthrough.
    #[must_use]
    pub fn next_states(self) -> &'static [SessionState] {
        use SessionState::{
            ApplyingTransient, AwaitingApproval, BackingUp, CompletedRestored, CompletedVerified,
            Connecting, Disconnected, Identifying, Planning, Rebooting, Reconnecting, Recovering,
            Saving, SnapshotRead, StateUnknownRecoveryRequired,
            TransientWritePendingReconcileOnResume, Verifying,
        };
        match self {
            Disconnected => &[Connecting],
            Connecting => &[Identifying, Disconnected, StateUnknownRecoveryRequired],
            Identifying => &[SnapshotRead, Disconnected, StateUnknownRecoveryRequired],
            SnapshotRead => &[Planning, Disconnected, StateUnknownRecoveryRequired],
            Planning => &[AwaitingApproval, StateUnknownRecoveryRequired],
            AwaitingApproval => &[BackingUp, StateUnknownRecoveryRequired],
            BackingUp => &[ApplyingTransient, StateUnknownRecoveryRequired],
            ApplyingTransient => &[
                TransientWritePendingReconcileOnResume,
                Saving,
                Recovering,
                StateUnknownRecoveryRequired,
            ],
            TransientWritePendingReconcileOnResume => {
                &[Saving, Recovering, StateUnknownRecoveryRequired]
            }
            Saving => &[Rebooting, Recovering, StateUnknownRecoveryRequired],
            Rebooting => &[Reconnecting, Recovering, StateUnknownRecoveryRequired],
            Reconnecting => &[Verifying, Recovering, StateUnknownRecoveryRequired],
            Verifying => &[CompletedVerified, Recovering, StateUnknownRecoveryRequired],
            Recovering => &[
                Saving,
                Rebooting,
                Reconnecting,
                CompletedRestored,
                StateUnknownRecoveryRequired,
            ],
            CompletedVerified | CompletedRestored | StateUnknownRecoveryRequired => &[],
        }
    }

    /// Whether a transition from `self` to `next` is allowed.
    #[must_use]
    pub fn may_transition_to(self, next: SessionState) -> bool {
        self.next_states().contains(&next)
    }
}

/// A session whose state can only change through validated transitions.
#[derive(Debug, Clone)]
pub struct Session {
    state: SessionState,
    history: Vec<SessionState>,
}

impl Session {
    /// A new session in [`SessionState::Disconnected`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SessionState::Disconnected,
            history: vec![SessionState::Disconnected],
        }
    }

    /// The current state.
    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// The ordered history of states this session has occupied.
    #[must_use]
    pub fn history(&self) -> &[SessionState] {
        &self.history
    }

    /// Attempt a transition, recording it on success.
    ///
    /// # Errors
    /// [`InvalidTransition`] if `next` is not reachable in one step from the current state.
    pub fn transition_to(&mut self, next: SessionState) -> Result<(), InvalidTransition> {
        if self.state.may_transition_to(next) {
            self.state = next;
            self.history.push(next);
            Ok(())
        } else {
            Err(InvalidTransition {
                from: self.state,
                to: next,
            })
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// A rejected state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTransition {
    /// The state the session was in.
    pub from: SessionState,
    /// The state that was rejected.
    pub to: SessionState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_writes_are_permitted_during_identification() {
        assert!(!SessionState::Identifying.writes_permitted());
        assert!(!SessionState::SnapshotRead.writes_permitted());
        assert!(SessionState::ApplyingTransient.writes_permitted());
    }

    #[test]
    fn the_happy_path_is_a_valid_walk() {
        let mut s = Session::new();
        for next in [
            SessionState::Connecting,
            SessionState::Identifying,
            SessionState::SnapshotRead,
            SessionState::Planning,
            SessionState::AwaitingApproval,
            SessionState::BackingUp,
            SessionState::ApplyingTransient,
            SessionState::Saving,
            SessionState::Rebooting,
            SessionState::Reconnecting,
            SessionState::Verifying,
            SessionState::CompletedVerified,
        ] {
            s.transition_to(next).expect("valid step");
        }
        assert_eq!(s.state(), SessionState::CompletedVerified);
        assert!(s.state().is_terminal());
    }

    #[test]
    fn an_illegal_transition_is_rejected() {
        let mut s = Session::new();
        assert_eq!(
            s.transition_to(SessionState::CompletedVerified),
            Err(InvalidTransition {
                from: SessionState::Disconnected,
                to: SessionState::CompletedVerified,
            }),
        );
    }

    #[test]
    fn any_write_phase_can_drop_to_state_unknown() {
        assert!(SessionState::Saving.may_transition_to(SessionState::StateUnknownRecoveryRequired));
        assert!(SessionState::Verifying.may_transition_to(SessionState::Recovering));
    }
}
