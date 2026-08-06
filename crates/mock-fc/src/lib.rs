#![forbid(unsafe_code)]

//! # `ade-mock-fc` — deterministic mock flight controller (M1)
//!
//! A deterministic in-memory model of a Betaflight 4.5.5 flight controller, implementing
//! **only** the M1 contracts recorded under `provenance/records/`. It is a model of our own
//! records, so its results are `MOCK_EXERCISED`, **never** `HARDWARE_OBSERVED`; it is not
//! independent evidence that the upstream source is correct.
//!
//! It models the split between transient RAM configuration and persisted EEPROM: a
//! `MSP_SET_BEEPER_CONFIG` write changes RAM only, `MSP_EEPROM_WRITE` commits RAM to EEPROM,
//! and `MSP_REBOOT` reloads RAM from EEPROM — so an unsaved transient write is lost on reboot.

use ade_protocol_msp::{
    BeeperConfigSnapshot, CommandId, Direction, Frame, decode_frame, encode_frame,
};
use ade_transport::{FrameResponder, TransportError};

/// The classification of a mock result. Deliberately not "HARDWARE_OBSERVED".
pub const RESULT_CLASS: &str = "MOCK_EXERCISED";

/// The deterministic mock flight-controller state.
#[derive(Debug, Clone)]
pub struct MockFc {
    ram: BeeperConfigSnapshot,
    eeprom: BeeperConfigSnapshot,
    armed: bool,
    reboot_generation: u32,
    board_identifier: [u8; 4],
    variant: [u8; 4],
    fc_version: (u8, u8, u8),
    api: (u8, u8, u8),
}

impl MockFc {
    /// A mock in its power-on state with the given initial persisted beeper configuration.
    #[must_use]
    pub fn new(initial: BeeperConfigSnapshot) -> Self {
        Self {
            ram: initial.clone(),
            eeprom: initial,
            armed: false,
            reboot_generation: 0,
            board_identifier: *b"S405",
            variant: *b"BTFL",
            fc_version: (4, 5, 5),
            api: (0, 1, 46),
        }
    }

    /// Arm the mock (used to exercise the "EEPROM write refused while armed" contract).
    pub fn set_armed(&mut self, armed: bool) {
        self.armed = armed;
    }

    /// The current RAM beeper configuration.
    #[must_use]
    pub fn ram(&self) -> &BeeperConfigSnapshot {
        &self.ram
    }

    /// The persisted (EEPROM) beeper configuration.
    #[must_use]
    pub fn eeprom(&self) -> &BeeperConfigSnapshot {
        &self.eeprom
    }

    /// How many reboots have occurred.
    #[must_use]
    pub fn reboot_generation(&self) -> u32 {
        self.reboot_generation
    }

    /// Replace the board identifier (used to model a device returning with a new identity).
    pub fn set_board_identifier(&mut self, id: [u8; 4]) {
        self.board_identifier = id;
    }

    fn board_info_payload(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.board_identifier);
        out.extend_from_slice(&0u16.to_le_bytes()); // hardware revision
        out.push(0); // fc type: 0 == FC
        out.push(0); // target capabilities
        let target = b"SPEEDYBEEF405V4";
        out.push(target.len() as u8);
        out.extend_from_slice(target);
        let board = b"SpeedyBee F405 V4";
        out.push(board.len() as u8);
        out.extend_from_slice(board);
        let manufacturer = b"SPB";
        out.push(manufacturer.len() as u8);
        out.extend_from_slice(manufacturer);
        out.extend_from_slice(&[0u8; 32]); // signature
        out
    }

    fn reply(command: CommandId, payload: &[u8]) -> Result<Vec<u8>, TransportError> {
        encode_frame(Direction::Reply, command, payload).map_err(TransportError::Malformed)
    }

    fn error_reply(command: CommandId) -> Result<Vec<u8>, TransportError> {
        encode_frame(Direction::Error, command, &[]).map_err(TransportError::Malformed)
    }

    fn handle(&mut self, frame: &Frame) -> Result<Vec<u8>, TransportError> {
        let Some(command) = frame.known_command() else {
            return Err(TransportError::UnexpectedFrame);
        };
        match command {
            CommandId::ApiVersion => Self::reply(command, &[self.api.0, self.api.1, self.api.2]),
            CommandId::FcVariant => Self::reply(command, &self.variant),
            CommandId::FcVersion => Self::reply(
                command,
                &[self.fc_version.0, self.fc_version.1, self.fc_version.2],
            ),
            CommandId::BoardInfo => Self::reply(command, &self.board_info_payload()),
            CommandId::BeeperConfig => Self::reply(command, &self.ram.to_reply_payload()),
            CommandId::SetBeeperConfig => {
                let payload = frame.payload();
                // Mirror the recorded 4-byte-safe write: beeper_off_flags is updated; the
                // DShot beacon fields are touched only if extra bytes are present.
                if payload.len() >= 4 {
                    self.ram.beeper_off_flags =
                        u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
                }
                if payload.len() >= 5 {
                    self.ram.dshot_beacon_tone = payload[4];
                }
                if payload.len() >= 9 {
                    self.ram.dshot_beacon_off_flags =
                        u32::from_le_bytes([payload[5], payload[6], payload[7], payload[8]]);
                }
                Self::reply(command, &[])
            }
            CommandId::EepromWrite => {
                if self.armed {
                    return Self::error_reply(command);
                }
                self.eeprom = self.ram.clone();
                Self::reply(command, &[])
            }
            CommandId::Reboot => {
                let mode = frame.payload().first().copied().unwrap_or(0);
                self.reboot_generation += 1;
                // RAM is reloaded from EEPROM: an unsaved transient write is lost.
                self.ram = self.eeprom.clone();
                Self::reply(command, &[mode])
            }
        }
    }
}

impl FrameResponder for MockFc {
    fn respond(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        let frame = decode_frame(request).map_err(TransportError::Malformed)?;
        self.handle(&frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_protocol_msp::{
        ApiVersion, BeeperConfigSnapshot, BoardInfo, CommandId, Direction, SYSTEM_INIT_OFF_MASK,
        SetBeeperConfig, decode_frame, encode_frame,
    };

    fn snapshot(flags: u32) -> BeeperConfigSnapshot {
        BeeperConfigSnapshot {
            beeper_off_flags: flags,
            dshot_beacon_tone: 7,
            dshot_beacon_off_flags: 0x0000_0002,
        }
    }

    fn read(command: CommandId) -> Vec<u8> {
        encode_frame(Direction::Request, command, &[]).unwrap()
    }

    #[test]
    fn identity_replies_are_the_recorded_values() {
        let mut fc = MockFc::new(snapshot(0));
        let av = ApiVersion::from_reply(
            &decode_frame(&fc.respond(&read(CommandId::ApiVersion)).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            av,
            ApiVersion {
                protocol_version: 0,
                api_major: 1,
                api_minor: 46
            }
        );
        let bi = BoardInfo::from_reply(
            &decode_frame(&fc.respond(&read(CommandId::BoardInfo)).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(bi.target_name, "SPEEDYBEEF405V4");
    }

    #[test]
    fn four_byte_write_changes_only_beeper_off_flags() {
        let mut fc = MockFc::new(snapshot(0));
        let before = fc.ram().clone();
        fc.respond(
            &SetBeeperConfig::new(SYSTEM_INIT_OFF_MASK)
                .encode_request()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(fc.ram().beeper_off_flags, SYSTEM_INIT_OFF_MASK);
        // DShot fields untouched.
        assert_eq!(fc.ram().dshot_beacon_tone, before.dshot_beacon_tone);
        assert_eq!(
            fc.ram().dshot_beacon_off_flags,
            before.dshot_beacon_off_flags
        );
    }

    #[test]
    fn unsaved_transient_write_is_lost_on_reboot_but_saved_survives() {
        let mut fc = MockFc::new(snapshot(0));
        // Transient write, then reboot without saving -> RAM reverts to EEPROM (0).
        fc.respond(
            &SetBeeperConfig::new(SYSTEM_INIT_OFF_MASK)
                .encode_request()
                .unwrap(),
        )
        .unwrap();
        fc.respond(&read(CommandId::Reboot)).unwrap();
        assert_eq!(fc.ram().beeper_off_flags, 0);
        assert_eq!(fc.reboot_generation(), 1);
        // Write, save, then reboot -> survives.
        fc.respond(
            &SetBeeperConfig::new(SYSTEM_INIT_OFF_MASK)
                .encode_request()
                .unwrap(),
        )
        .unwrap();
        fc.respond(&read(CommandId::EepromWrite)).unwrap();
        fc.respond(&read(CommandId::Reboot)).unwrap();
        assert_eq!(fc.ram().beeper_off_flags, SYSTEM_INIT_OFF_MASK);
    }

    #[test]
    fn eeprom_write_is_refused_while_armed() {
        let mut fc = MockFc::new(snapshot(0));
        fc.set_armed(true);
        let reply = fc.respond(&read(CommandId::EepromWrite)).unwrap();
        let frame = decode_frame(&reply).unwrap();
        assert_eq!(frame.direction, Direction::Error);
    }
}
