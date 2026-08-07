//! Product-level input and responsibility contracts.
//!
//! These types deliberately stop before protocol values and write authority. They record
//! what the user is allowed to choose and what the deterministic engines must derive later.
//! No type in this module can send a command, select a UART, or approve a hardware write.

use ade_safety::RecoveryClass;

/// The flight behaviour selected by the user in product language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightIntent {
    Cinematic,
    Freestyle,
    Racing,
    LongRange,
}

/// A configuration domain the finished product must own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingDomain {
    Power,
    Motors,
    EscProtocol,
    ReceiverTransport,
    ChannelOrder,
    ControlFunctionAssignments,
    Rates,
    Filters,
    FlightControl,
    Failsafe,
    GpsAndNavigation,
    VideoAndOsd,
    AlertsAndAccessories,
    Firmware,
}

/// Who decides a supported configuration domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Responsibility {
    /// The program derives, validates, applies and verifies the setting.
    ProgramAutomatic,
    /// The user explicitly chooses which physical switch/button performs a function.
    UserManualControlAssignment,
}

impl SettingDomain {
    /// Return the binding product responsibility for this domain.
    #[must_use]
    pub const fn responsibility(self) -> Responsibility {
        match self {
            Self::ControlFunctionAssignments => Responsibility::UserManualControlAssignment,
            Self::Power
            | Self::Motors
            | Self::EscProtocol
            | Self::ReceiverTransport
            | Self::ChannelOrder
            | Self::Rates
            | Self::Filters
            | Self::FlightControl
            | Self::Failsafe
            | Self::GpsAndNavigation
            | Self::VideoAndOsd
            | Self::AlertsAndAccessories
            | Self::Firmware => Responsibility::ProgramAutomatic,
        }
    }
}

/// Exhaustive domain list used by responsibility and acceptance tests.
pub const ALL_SETTING_DOMAINS: [SettingDomain; 14] = [
    SettingDomain::Power,
    SettingDomain::Motors,
    SettingDomain::EscProtocol,
    SettingDomain::ReceiverTransport,
    SettingDomain::ChannelOrder,
    SettingDomain::ControlFunctionAssignments,
    SettingDomain::Rates,
    SettingDomain::Filters,
    SettingDomain::FlightControl,
    SettingDomain::Failsafe,
    SettingDomain::GpsAndNavigation,
    SettingDomain::VideoAndOsd,
    SettingDomain::AlertsAndAccessories,
    SettingDomain::Firmware,
];

/// A component fact the user supplies because it cannot yet be discovered reliably.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredComponent {
    pub kind: ComponentKind,
    pub value: String,
}

/// Product-level component categories. They contain no firmware-specific setting names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    Frame,
    Battery,
    Motor,
    Propeller,
    Esc,
    RadioLink,
    VideoSystem,
    Gps,
}

/// A physical transmitter input observed during the guided assignment step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlInput {
    Switch(u8),
    Button(u8),
}

/// A function the user may deliberately bind to a physical input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFunction {
    Arm,
    FlightMode,
    Buzzer,
    Rescue,
    TurtleMode,
}

/// One explicit user decision; the program may validate it but never invent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualControlAssignment {
    pub input: ControlInput,
    pub function: ControlFunction,
}

/// Validated manual assignments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualControlAssignments(Vec<ManualControlAssignment>);

/// Evidence that one control assignment came from the validated user-owned collection.
///
/// There is no public constructor. A planner obtains it only through
/// [`ManualControlAssignments::confirmed_decision`], preserving the exact input/function
/// choice instead of replacing it with a boolean "user confirmed" claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserConfirmedControlDecision {
    assignment: ManualControlAssignment,
}

impl UserConfirmedControlDecision {
    /// The exact assignment the user chose.
    #[must_use]
    pub const fn assignment(self) -> ManualControlAssignment {
        self.assignment
    }
}

/// Why user-owned product input was rejected before planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductInputError {
    DuplicateControlInput(ControlInput),
    DuplicateControlFunction(ControlFunction),
}

impl ManualControlAssignments {
    /// Validate that one input cannot drive two functions and one function is not assigned
    /// twice. Empty assignments are allowed until the guided assignment step is complete.
    pub fn new(assignments: Vec<ManualControlAssignment>) -> Result<Self, ProductInputError> {
        for (index, assignment) in assignments.iter().enumerate() {
            for previous in &assignments[..index] {
                if previous.input == assignment.input {
                    return Err(ProductInputError::DuplicateControlInput(assignment.input));
                }
                if previous.function == assignment.function {
                    return Err(ProductInputError::DuplicateControlFunction(
                        assignment.function,
                    ));
                }
            }
        }
        Ok(Self(assignments))
    }

    /// Read the user-confirmed assignments without exposing a mutation shortcut.
    #[must_use]
    pub fn as_slice(&self) -> &[ManualControlAssignment] {
        &self.0
    }

    /// Produce typed decision evidence for one validated assignment.
    #[must_use]
    pub fn confirmed_decision(&self, index: usize) -> Option<UserConfirmedControlDecision> {
        self.0
            .get(index)
            .copied()
            .map(|assignment| UserConfirmedControlDecision { assignment })
    }
}

/// User-selected firmware acquisition route. Both variants carry verification metadata;
/// neither carries executable code, raw firmware bytes, or a host filesystem path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwareSource {
    TrustedDownload {
        release_id: String,
        expected_sha256: [u8; 32],
    },
    ManualLocalFile {
        display_name: String,
        sha256: [u8; 32],
    },
}

/// The complete product-level goal accepted by later deterministic planners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductGoal {
    pub components: Vec<DeclaredComponent>,
    pub flight_intent: FlightIntent,
    pub control_assignments: ManualControlAssignments,
    pub firmware_source: FirmwareSource,
}

/// A stable, pack-owned identifier for one technical setting.
///
/// The product surface uses domains and user goals; it never displays this numeric key.
/// Keeping the key numeric also prevents protocol/configurator setting names from leaking
/// into the ordinary interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingId {
    domain: SettingDomain,
    key: u16,
}

impl SettingId {
    /// Construct an internal setting identifier within a product domain.
    #[must_use]
    pub const fn new(domain: SettingDomain, key: u16) -> Self {
        Self { domain, key }
    }

    /// The product domain that owns this setting.
    #[must_use]
    pub const fn domain(self) -> SettingDomain {
        self.domain
    }

    /// The pack-owned numeric key. This is not a protocol command or raw address.
    #[must_use]
    pub const fn key(self) -> u16 {
        self.key
    }
}

/// A protocol-independent setting value used by deterministic planners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingValue {
    Boolean(bool),
    Signed(i64),
    Unsigned(u64),
    /// A pack-owned choice identifier; never a user-visible configurator string.
    Choice(u16),
    /// The exact transmitter assignment selected by the user, or no prior assignment.
    /// This value is valid only for [`SettingDomain::ControlFunctionAssignments`].
    ControlAssignment(Option<ManualControlAssignment>),
}

/// Where the decision represented by a setting delta came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionSource {
    /// The deterministic program derived the value from verified facts and capability data.
    ProgramDerived,
    /// The user explicitly chose a transmitter control assignment in the guided UI.
    UserConfirmedControlAssignment(UserConfirmedControlDecision),
}

/// Why a generic product setting delta was rejected before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductPlanError {
    /// A value that did not change was presented as a delta.
    NoChange(SettingId),
    /// The decision source conflicts with the binding responsibility contract.
    ResponsibilityMismatch {
        setting: SettingId,
        source: DecisionSource,
    },
    /// A control-assignment delta did not reproduce the user's exact confirmed choice.
    UserDecisionMismatch {
        setting: SettingId,
        confirmed: ManualControlAssignment,
        proposed: SettingValue,
    },
    /// A control-assignment delta did not use the dedicated typed value on both sides.
    ControlAssignmentValueRequired(SettingId),
    /// A control-assignment value was used outside its one valid product domain.
    ControlAssignmentValueOutsideDomain(SettingId),
    /// A real delta did not declare a usable recovery posture.
    InvalidRecoveryClass(SettingId),
    /// A setting delta has no pinned provenance evidence.
    MissingProvenance(SettingId),
    /// A provenance id was present but blank.
    EmptyProvenanceId(SettingId),
    /// The same setting appeared more than once in one plan.
    DuplicateSetting(SettingId),
}

/// One protocol-independent setting delta in a product configuration plan.
///
/// Fields are private so callers cannot bypass the responsibility, recovery or provenance
/// checks. This type describes intent only; it carries no command bytes or write authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedSettingChange {
    setting: SettingId,
    before: SettingValue,
    after: SettingValue,
    source: DecisionSource,
    recovery: RecoveryClass,
    provenance_ids: Vec<String>,
}

impl PlannedSettingChange {
    /// Validate and construct one setting delta.
    ///
    /// # Errors
    /// Returns [`ProductPlanError`] when the delta is empty, violates the product
    /// responsibility boundary, lacks a usable recovery posture or lacks provenance.
    pub fn new(
        setting: SettingId,
        before: SettingValue,
        after: SettingValue,
        source: DecisionSource,
        recovery: RecoveryClass,
        provenance_ids: Vec<String>,
    ) -> Result<Self, ProductPlanError> {
        if before == after {
            return Err(ProductPlanError::NoChange(setting));
        }
        match (setting.domain().responsibility(), source, before, after) {
            (
                Responsibility::ProgramAutomatic,
                DecisionSource::ProgramDerived,
                previous,
                proposed,
            ) => {
                if matches!(previous, SettingValue::ControlAssignment(_))
                    || matches!(proposed, SettingValue::ControlAssignment(_))
                {
                    return Err(ProductPlanError::ControlAssignmentValueOutsideDomain(
                        setting,
                    ));
                }
            }
            (
                Responsibility::UserManualControlAssignment,
                DecisionSource::UserConfirmedControlAssignment(decision),
                SettingValue::ControlAssignment(_),
                SettingValue::ControlAssignment(Some(proposed)),
            ) if proposed == decision.assignment() => {}
            (
                Responsibility::UserManualControlAssignment,
                DecisionSource::UserConfirmedControlAssignment(decision),
                SettingValue::ControlAssignment(_),
                proposed,
            ) => {
                return Err(ProductPlanError::UserDecisionMismatch {
                    setting,
                    confirmed: decision.assignment(),
                    proposed,
                });
            }
            (
                Responsibility::UserManualControlAssignment,
                DecisionSource::UserConfirmedControlAssignment(_),
                _,
                _,
            ) => return Err(ProductPlanError::ControlAssignmentValueRequired(setting)),
            _ => return Err(ProductPlanError::ResponsibilityMismatch { setting, source }),
        }
        if matches!(
            recovery,
            RecoveryClass::NotApplicableNoWrite | RecoveryClass::StateUnknownRecoveryRequired
        ) {
            return Err(ProductPlanError::InvalidRecoveryClass(setting));
        }
        if provenance_ids.is_empty() {
            return Err(ProductPlanError::MissingProvenance(setting));
        }
        if provenance_ids.iter().any(|id| id.trim().is_empty()) {
            return Err(ProductPlanError::EmptyProvenanceId(setting));
        }
        Ok(Self {
            setting,
            before,
            after,
            source,
            recovery,
            provenance_ids,
        })
    }

    #[must_use]
    pub const fn setting(&self) -> SettingId {
        self.setting
    }

    #[must_use]
    pub const fn before(&self) -> SettingValue {
        self.before
    }

    #[must_use]
    pub const fn after(&self) -> SettingValue {
        self.after
    }

    #[must_use]
    pub const fn source(&self) -> DecisionSource {
        self.source
    }

    #[must_use]
    pub const fn recovery(&self) -> RecoveryClass {
        self.recovery
    }

    #[must_use]
    pub fn provenance_ids(&self) -> &[String] {
        &self.provenance_ids
    }
}

/// A validated product plan. An empty change list is the only no-op representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductConfigurationPlan {
    changes: Vec<PlannedSettingChange>,
}

impl ProductConfigurationPlan {
    /// Build a plan and reject duplicate setting identifiers.
    ///
    /// # Errors
    /// Returns [`ProductPlanError::DuplicateSetting`] if a setting occurs more than once.
    pub fn new(changes: Vec<PlannedSettingChange>) -> Result<Self, ProductPlanError> {
        for (index, change) in changes.iter().enumerate() {
            if changes[..index]
                .iter()
                .any(|previous| previous.setting() == change.setting())
            {
                return Err(ProductPlanError::DuplicateSetting(change.setting()));
            }
        }
        Ok(Self { changes })
    }

    /// Ordered, validated setting deltas.
    #[must_use]
    pub fn changes(&self) -> &[PlannedSettingChange] {
        &self.changes
    }

    /// Whether the observed configuration already satisfies the goal.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.changes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance() -> Vec<String> {
        vec!["test-provenance-record".to_owned()]
    }

    fn user_decision() -> UserConfirmedControlDecision {
        ManualControlAssignments::new(vec![ManualControlAssignment {
            input: ControlInput::Switch(1),
            function: ControlFunction::Arm,
        }])
        .unwrap()
        .confirmed_decision(0)
        .unwrap()
    }

    #[test]
    fn every_supported_domain_is_automatic_except_control_function_assignment() {
        let manual: Vec<_> = ALL_SETTING_DOMAINS
            .iter()
            .copied()
            .filter(|domain| domain.responsibility() == Responsibility::UserManualControlAssignment)
            .collect();
        assert_eq!(manual, vec![SettingDomain::ControlFunctionAssignments]);
        assert_eq!(ALL_SETTING_DOMAINS.len() - manual.len(), 13);
    }

    #[test]
    fn manual_assignments_preserve_the_users_exact_choice() {
        let assignments = ManualControlAssignments::new(vec![ManualControlAssignment {
            input: ControlInput::Switch(2),
            function: ControlFunction::Arm,
        }])
        .unwrap();
        assert_eq!(
            assignments.as_slice(),
            &[ManualControlAssignment {
                input: ControlInput::Switch(2),
                function: ControlFunction::Arm,
            }]
        );
    }

    #[test]
    fn duplicate_inputs_and_functions_are_refused() {
        let same_input = ManualControlAssignments::new(vec![
            ManualControlAssignment {
                input: ControlInput::Switch(1),
                function: ControlFunction::Arm,
            },
            ManualControlAssignment {
                input: ControlInput::Switch(1),
                function: ControlFunction::Buzzer,
            },
        ]);
        assert_eq!(
            same_input,
            Err(ProductInputError::DuplicateControlInput(
                ControlInput::Switch(1)
            ))
        );

        let same_function = ManualControlAssignments::new(vec![
            ManualControlAssignment {
                input: ControlInput::Switch(1),
                function: ControlFunction::Arm,
            },
            ManualControlAssignment {
                input: ControlInput::Button(1),
                function: ControlFunction::Arm,
            },
        ]);
        assert_eq!(
            same_function,
            Err(ProductInputError::DuplicateControlFunction(
                ControlFunction::Arm
            ))
        );
    }

    #[test]
    fn product_goal_uses_product_language_and_local_firmware_metadata() {
        let goal = ProductGoal {
            components: vec![DeclaredComponent {
                kind: ComponentKind::Battery,
                value: "6S".to_string(),
            }],
            flight_intent: FlightIntent::Freestyle,
            control_assignments: ManualControlAssignments::new(Vec::new()).unwrap(),
            firmware_source: FirmwareSource::ManualLocalFile {
                display_name: "firmware.bin".to_string(),
                sha256: [7; 32],
            },
        };
        assert_eq!(goal.flight_intent, FlightIntent::Freestyle);
        assert_eq!(goal.components[0].value, "6S");
        assert!(matches!(
            goal.firmware_source,
            FirmwareSource::ManualLocalFile { sha256, .. } if sha256 == [7; 32]
        ));
    }

    #[test]
    fn automatic_settings_accept_only_program_derived_decisions() {
        let setting = SettingId::new(SettingDomain::Filters, 7);
        let manual_source = DecisionSource::UserConfirmedControlAssignment(user_decision());
        let change = PlannedSettingChange::new(
            setting,
            SettingValue::Unsigned(90),
            SettingValue::Unsigned(100),
            DecisionSource::ProgramDerived,
            RecoveryClass::RestoreFromBackupSupported,
            provenance(),
        )
        .unwrap();
        assert_eq!(change.setting(), setting);
        assert_eq!(change.source(), DecisionSource::ProgramDerived);

        assert_eq!(
            PlannedSettingChange::new(
                setting,
                SettingValue::Unsigned(90),
                SettingValue::Unsigned(100),
                manual_source,
                RecoveryClass::RestoreFromBackupSupported,
                provenance(),
            ),
            Err(ProductPlanError::ResponsibilityMismatch {
                setting,
                source: manual_source,
            })
        );
    }

    #[test]
    fn control_assignments_can_only_preserve_an_explicit_user_decision() {
        let setting = SettingId::new(SettingDomain::ControlFunctionAssignments, 1);
        let assignments = ManualControlAssignments::new(vec![ManualControlAssignment {
            input: ControlInput::Switch(3),
            function: ControlFunction::Buzzer,
        }])
        .unwrap();
        let user_decision = assignments.confirmed_decision(0).unwrap();
        let exact_assignment = user_decision.assignment();
        assert!(
            PlannedSettingChange::new(
                setting,
                SettingValue::ControlAssignment(None),
                SettingValue::ControlAssignment(Some(exact_assignment)),
                DecisionSource::UserConfirmedControlAssignment(user_decision),
                RecoveryClass::RestoreFromBackupSupported,
                provenance(),
            )
            .is_ok()
        );
        assert_eq!(
            user_decision.assignment(),
            ManualControlAssignment {
                input: ControlInput::Switch(3),
                function: ControlFunction::Buzzer,
            }
        );
        assert_eq!(
            PlannedSettingChange::new(
                setting,
                SettingValue::ControlAssignment(None),
                SettingValue::ControlAssignment(Some(exact_assignment)),
                DecisionSource::ProgramDerived,
                RecoveryClass::RestoreFromBackupSupported,
                provenance(),
            ),
            Err(ProductPlanError::ResponsibilityMismatch {
                setting,
                source: DecisionSource::ProgramDerived,
            })
        );

        let invented = ManualControlAssignment {
            input: ControlInput::Button(9),
            function: ControlFunction::Rescue,
        };
        assert_eq!(
            PlannedSettingChange::new(
                setting,
                SettingValue::ControlAssignment(None),
                SettingValue::ControlAssignment(Some(invented)),
                DecisionSource::UserConfirmedControlAssignment(user_decision),
                RecoveryClass::RestoreFromBackupSupported,
                provenance(),
            ),
            Err(ProductPlanError::UserDecisionMismatch {
                setting,
                confirmed: exact_assignment,
                proposed: SettingValue::ControlAssignment(Some(invented)),
            })
        );

        assert_eq!(
            PlannedSettingChange::new(
                setting,
                SettingValue::Choice(1),
                SettingValue::ControlAssignment(Some(exact_assignment)),
                DecisionSource::UserConfirmedControlAssignment(user_decision),
                RecoveryClass::RestoreFromBackupSupported,
                provenance(),
            ),
            Err(ProductPlanError::ControlAssignmentValueRequired(setting))
        );
    }

    #[test]
    fn control_assignment_values_cannot_leak_into_automatic_domains() {
        let setting = SettingId::new(SettingDomain::Filters, 9);
        assert_eq!(
            PlannedSettingChange::new(
                setting,
                SettingValue::Unsigned(10),
                SettingValue::ControlAssignment(Some(user_decision().assignment())),
                DecisionSource::ProgramDerived,
                RecoveryClass::RestoreFromBackupSupported,
                provenance(),
            ),
            Err(ProductPlanError::ControlAssignmentValueOutsideDomain(
                setting
            ))
        );
    }

    #[test]
    fn a_delta_requires_change_recovery_and_provenance() {
        let setting = SettingId::new(SettingDomain::Rates, 3);
        assert_eq!(
            PlannedSettingChange::new(
                setting,
                SettingValue::Unsigned(10),
                SettingValue::Unsigned(10),
                DecisionSource::ProgramDerived,
                RecoveryClass::RestoreFromBackupSupported,
                provenance(),
            ),
            Err(ProductPlanError::NoChange(setting))
        );
        assert_eq!(
            PlannedSettingChange::new(
                setting,
                SettingValue::Unsigned(10),
                SettingValue::Unsigned(11),
                DecisionSource::ProgramDerived,
                RecoveryClass::NotApplicableNoWrite,
                provenance(),
            ),
            Err(ProductPlanError::InvalidRecoveryClass(setting))
        );
        assert_eq!(
            PlannedSettingChange::new(
                setting,
                SettingValue::Unsigned(10),
                SettingValue::Unsigned(11),
                DecisionSource::ProgramDerived,
                RecoveryClass::RestoreFromBackupSupported,
                Vec::new(),
            ),
            Err(ProductPlanError::MissingProvenance(setting))
        );
    }

    #[test]
    fn a_plan_has_one_delta_per_setting_and_one_noop_form() {
        let setting = SettingId::new(SettingDomain::Failsafe, 4);
        let change = PlannedSettingChange::new(
            setting,
            SettingValue::Boolean(false),
            SettingValue::Boolean(true),
            DecisionSource::ProgramDerived,
            RecoveryClass::AutomaticRollbackSupported,
            provenance(),
        )
        .unwrap();
        assert_eq!(
            ProductConfigurationPlan::new(vec![change.clone(), change]),
            Err(ProductPlanError::DuplicateSetting(setting))
        );

        let noop = ProductConfigurationPlan::new(Vec::new()).unwrap();
        assert!(noop.is_noop());
        assert!(noop.changes().is_empty());
    }
}
