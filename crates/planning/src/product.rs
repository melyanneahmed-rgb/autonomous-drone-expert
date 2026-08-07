//! Product-level input and responsibility contracts.
//!
//! These types deliberately stop before protocol values and write authority. They record
//! what the user is allowed to choose and what the deterministic engines must derive later.
//! No type in this module can send a command, select a UART, or approve a hardware write.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
