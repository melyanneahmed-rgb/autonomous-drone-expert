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
        // A zero signature: the mock never fabricates a per-unit signature, and downstream
        // identity never depends on it.
        out.extend_from_slice(&[0u8; 32]);
        // Complete API 1.46 tail so the parser consumes the whole payload (no silent tail):
        out.push(0); // mcu_type_id
        out.push(0); // configuration_state
        out.extend_from_slice(&0u16.to_le_bytes()); // gyro_sample_rate_hz
        out.extend_from_slice(&0u32.to_le_bytes()); // configuration_problems
        out.push(0); // spi_device_count
        out.push(0); // i2c_device_count
        out
    }

    fn reply(command: CommandId, payload: &[u8]) -> Result<Vec<u8>, TransportError> {
        encode_frame(Direction::Reply, command, payload).map_err(TransportError::Malformed)
    }

    fn error_reply(command: CommandId) -> Result<Vec<u8>, TransportError> {
        encode_frame(Direction::Error, command, &[]).map_err(TransportError::Malformed)
    }

    fn handle(&mut self, frame: &Frame) -> Result<Vec<u8>, TransportError> {
        // The mock only ever processes host *requests*. A reply/error frame arriving as a
        // "request" is a protocol violation and is refused before any state is examined.
        if frame.direction != Direction::Request {
            return Err(TransportError::UnexpectedFrame);
        }
        let Some(command) = frame.known_command() else {
            return Err(TransportError::UnexpectedFrame);
        };
        let payload = frame.payload();
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
                // The M1 write is EXACTLY four bytes — `beeper_off_flags` only. Any other
                // length is a protocol violation: it is NACKed with an MSP error reply and
                // RAM is left untouched. The DShot beacon fields are NEVER written on this
                // path, so a longer frame can never smuggle in a DShot change.
                if payload.len() != 4 {
                    return Self::error_reply(command);
                }
                self.ram.beeper_off_flags =
                    u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
                Self::reply(command, &[])
            }
            CommandId::EepromWrite => {
                // An EEPROM write carries no payload; anything else is refused without a
                // commit. A refused frame never changes EEPROM.
                if !payload.is_empty() {
                    return Self::error_reply(command);
                }
                if self.armed {
                    return Self::error_reply(command);
                }
                self.eeprom = self.ram.clone();
                Self::reply(command, &[])
            }
            CommandId::Reboot => {
                // A normal reboot carries no payload. Any payload is refused rather than
                // silently accepting a stray reboot-mode byte, and a refused reboot changes
                // neither RAM nor the reboot generation.
                if !payload.is_empty() {
                    return Self::error_reply(command);
                }
                self.reboot_generation += 1;
                // RAM is reloaded from EEPROM: an unsaved transient write is lost.
                self.ram = self.eeprom.clone();
                Self::reply(command, &[])
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

    /// A frame whose direction is not `Request` is refused outright.
    #[test]
    fn a_non_request_frame_is_refused() {
        let mut fc = MockFc::new(snapshot(0));
        let reply_frame =
            encode_frame(Direction::Reply, CommandId::ApiVersion, &[0, 1, 46]).unwrap();
        assert_eq!(
            fc.respond(&reply_frame),
            Err(TransportError::UnexpectedFrame)
        );
        let error_frame = encode_frame(Direction::Error, CommandId::ApiVersion, &[]).unwrap();
        assert_eq!(
            fc.respond(&error_frame),
            Err(TransportError::UnexpectedFrame)
        );
    }

    fn assert_error_reply(reply: &[u8], command: CommandId) {
        let frame = decode_frame(reply).unwrap();
        assert_eq!(frame.direction, Direction::Error);
        assert_eq!(frame.known_command(), Some(command));
    }

    /// Every `MSP_SET_BEEPER_CONFIG` payload length other than exactly four is NACKed, and
    /// RAM — including the DShot beacon fields — is unchanged after each rejected request.
    #[test]
    fn set_beeper_config_of_any_non_four_length_is_refused_and_ram_is_unchanged() {
        let mut fc = MockFc::new(snapshot(0xAA));
        let before = fc.ram().clone();
        for len in [0usize, 1, 2, 3, 5, 8, 9, 10] {
            let frame = encode_frame(
                Direction::Request,
                CommandId::SetBeeperConfig,
                &vec![0x55u8; len],
            )
            .unwrap();
            let reply = fc.respond(&frame).unwrap();
            assert_error_reply(&reply, CommandId::SetBeeperConfig);
            assert_eq!(
                fc.ram(),
                &before,
                "RAM changed after a rejected len {len} write"
            );
            // DShot beacon fields, specifically, are never touched.
            assert_eq!(fc.ram().dshot_beacon_tone, before.dshot_beacon_tone);
            assert_eq!(
                fc.ram().dshot_beacon_off_flags,
                before.dshot_beacon_off_flags
            );
        }
        // The exactly-four-byte write still works and touches only beeper_off_flags.
        let ok = SetBeeperConfig::new(SYSTEM_INIT_OFF_MASK)
            .encode_request()
            .unwrap();
        fc.respond(&ok).unwrap();
        assert_eq!(fc.ram().beeper_off_flags, SYSTEM_INIT_OFF_MASK);
        assert_eq!(fc.ram().dshot_beacon_tone, before.dshot_beacon_tone);
        assert_eq!(
            fc.ram().dshot_beacon_off_flags,
            before.dshot_beacon_off_flags
        );
    }

    /// An `MSP_EEPROM_WRITE` with any payload is refused and EEPROM is not committed.
    #[test]
    fn eeprom_write_with_a_payload_is_refused_and_eeprom_is_unchanged() {
        let mut fc = MockFc::new(snapshot(0));
        // Make RAM differ from EEPROM so a wrongful commit would be observable.
        fc.respond(
            &SetBeeperConfig::new(SYSTEM_INIT_OFF_MASK)
                .encode_request()
                .unwrap(),
        )
        .unwrap();
        let eeprom_before = fc.eeprom().clone();
        let frame = encode_frame(Direction::Request, CommandId::EepromWrite, &[0x01]).unwrap();
        let reply = fc.respond(&frame).unwrap();
        assert_error_reply(&reply, CommandId::EepromWrite);
        assert_eq!(fc.eeprom(), &eeprom_before, "EEPROM must not be committed");
    }

    /// An `MSP_REBOOT` with any payload is refused and neither RAM nor the reboot generation
    /// changes.
    #[test]
    fn reboot_with_a_payload_is_refused_and_nothing_changes() {
        let mut fc = MockFc::new(snapshot(0));
        // A pending transient write that a wrongful reboot would discard.
        fc.respond(
            &SetBeeperConfig::new(SYSTEM_INIT_OFF_MASK)
                .encode_request()
                .unwrap(),
        )
        .unwrap();
        let ram_before = fc.ram().clone();
        let gen_before = fc.reboot_generation();
        let frame = encode_frame(Direction::Request, CommandId::Reboot, &[0x00]).unwrap();
        let reply = fc.respond(&frame).unwrap();
        assert_error_reply(&reply, CommandId::Reboot);
        assert_eq!(
            fc.ram(),
            &ram_before,
            "RAM must be untouched by a rejected reboot"
        );
        assert_eq!(fc.reboot_generation(), gen_before);
    }
}
