#![forbid(unsafe_code)]

//! # `ade-protocol-msp` — minimal MSP codec for the M1 beeper slice
//!
//! Clean-room implementation of exactly the MSP surface the M1 vertical slice needs,
//! built only from facts recorded under `provenance/records/` at the pinned Betaflight
//! tag `4.5.5`. No upstream code, comments, tables or fixtures were copied.
//!
//! Scope (M1 only): the MSPv1 serial frame, the eight commands used by the slice
//! (`MSP_API_VERSION`, `MSP_FC_VARIANT`, `MSP_FC_VERSION`, `MSP_BOARD_INFO`,
//! `MSP_BEEPER_CONFIG`, `MSP_SET_BEEPER_CONFIG`, `MSP_EEPROM_WRITE`, `MSP_REBOOT`), typed
//! payloads for them, request/reply correlation, and strict rejection of malformed input.
//!
//! Guarantees:
//! * never panics on arbitrary input — every decode returns a [`Result`];
//! * bounded frames — the MSPv1 size field is a single byte, so a payload is at most
//!   [`MAX_PAYLOAD`] bytes and every read is bounds-checked;
//! * no raw payload is ever logged or formatted for humans by this crate;
//! * the write surface for the beeper is [`SetBeeperConfig`], which encodes exactly four
//!   bytes and cannot express an arbitrary payload.

/// MSPv1 preamble byte `$`.
pub const PREAMBLE_DOLLAR: u8 = b'$';
/// MSPv1 preamble byte `M`.
pub const PREAMBLE_M: u8 = b'M';
/// Largest payload an MSPv1 frame can carry (the size field is one byte).
pub const MAX_PAYLOAD: usize = u8::MAX as usize;
/// Bytes of framing overhead around a payload: `$ M dir size cmd … checksum`.
pub const FRAME_OVERHEAD: usize = 6;

/// The `beeper_off_flags` bit that gates the power-on initialisation beep.
///
/// Recorded fact `bf-4.5.5-beeper-system-init-bit`: `BEEPER_GET_FLAG(mode) = 1 << (mode - 1)`
/// and `BEEPER_SYSTEM_INIT` has ordinal 17, so the mask is `1 << 16`. A set bit **disables**
/// the condition; a clear bit **allows** it.
pub const SYSTEM_INIT_OFF_MASK: u32 = 1 << 16;

/// Direction byte of an MSPv1 frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `<` — a request from the host to the flight controller.
    Request,
    /// `>` — a normal reply from the flight controller.
    Reply,
    /// `!` — an error reply from the flight controller.
    Error,
}

impl Direction {
    /// The on-wire byte for this direction.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        match self {
            Direction::Request => b'<',
            Direction::Reply => b'>',
            Direction::Error => b'!',
        }
    }

    /// Parse a direction byte, rejecting anything else.
    ///
    /// # Errors
    /// Returns [`MspError::BadDirection`] for any byte other than `<`, `>` or `!`.
    pub const fn from_byte(byte: u8) -> Result<Self, MspError> {
        match byte {
            b'<' => Ok(Direction::Request),
            b'>' => Ok(Direction::Reply),
            b'!' => Ok(Direction::Error),
            other => Err(MspError::BadDirection(other)),
        }
    }
}

/// The MSP commands used by the M1 slice. Each maps to a `PINNED_SOURCE_RECORDED` fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    /// `MSP_API_VERSION` (1).
    ApiVersion,
    /// `MSP_FC_VARIANT` (2).
    FcVariant,
    /// `MSP_FC_VERSION` (3).
    FcVersion,
    /// `MSP_BOARD_INFO` (4).
    BoardInfo,
    /// `MSP_REBOOT` (68).
    Reboot,
    /// `MSP_BEEPER_CONFIG` (184).
    BeeperConfig,
    /// `MSP_SET_BEEPER_CONFIG` (185).
    SetBeeperConfig,
    /// `MSP_EEPROM_WRITE` (250).
    EepromWrite,
}

impl CommandId {
    /// The numeric command identifier.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            CommandId::ApiVersion => 1,
            CommandId::FcVariant => 2,
            CommandId::FcVersion => 3,
            CommandId::BoardInfo => 4,
            CommandId::Reboot => 68,
            CommandId::BeeperConfig => 184,
            CommandId::SetBeeperConfig => 185,
            CommandId::EepromWrite => 250,
        }
    }

    /// Recognise a known M1 command, if any. Unknown commands stay as their raw byte on the
    /// [`Frame`]; this crate never invents a command it has no provenance record for.
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(CommandId::ApiVersion),
            2 => Some(CommandId::FcVariant),
            3 => Some(CommandId::FcVersion),
            4 => Some(CommandId::BoardInfo),
            68 => Some(CommandId::Reboot),
            184 => Some(CommandId::BeeperConfig),
            185 => Some(CommandId::SetBeeperConfig),
            250 => Some(CommandId::EepromWrite),
            _ => None,
        }
    }
}

/// Errors from encoding, decoding or interpreting an MSP frame. Never panics; carries only
/// structural information (counts, bytes, command numbers) — never raw payload content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MspError {
    /// A payload longer than [`MAX_PAYLOAD`] cannot be framed in MSPv1.
    PayloadTooLong(usize),
    /// Fewer bytes than the frame claims (or fewer than the minimum frame).
    Truncated { needed: usize, got: usize },
    /// Trailing bytes after a complete single frame.
    TrailingBytes { frame_len: usize, got: usize },
    /// The `$M` preamble was missing.
    BadPreamble,
    /// The direction byte was not `<`, `>` or `!`.
    BadDirection(u8),
    /// The checksum did not match.
    BadChecksum { expected: u8, found: u8 },
    /// A typed decode was asked to read a different command than the frame carried.
    WrongCommand { expected: u8, found: u8 },
    /// A typed decode expected a reply/error but the frame was a request (or vice versa).
    WrongDirection,
    /// A fixed-length payload had the wrong length.
    WrongLength { expected: usize, got: usize },
    /// A variable-length payload declared a field longer than the bytes that remain.
    FieldOverrun { field_len: usize, remaining: usize },
}

/// A decoded MSPv1 frame. The payload is retained as bytes; typed accessors interpret it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Frame direction.
    pub direction: Direction,
    /// The raw command byte as it appeared on the wire.
    pub command: u8,
    payload: Vec<u8>,
}

impl Frame {
    /// The recognised command, if this crate has a record for it.
    #[must_use]
    pub fn known_command(&self) -> Option<CommandId> {
        CommandId::from_u8(self.command)
    }

    /// The payload length in bytes (a count only — never the content).
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }

    /// Borrow the payload bytes. Callers must not log them.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    fn expect(&self, command: CommandId, want_reply: bool) -> Result<(), MspError> {
        if self.command != command.as_u8() {
            return Err(MspError::WrongCommand {
                expected: command.as_u8(),
                found: self.command,
            });
        }
        let is_reply = matches!(self.direction, Direction::Reply);
        if is_reply != want_reply {
            return Err(MspError::WrongDirection);
        }
        Ok(())
    }
}

/// XOR checksum over the size byte, command byte and payload (MSPv1).
fn checksum(size: u8, command: u8, payload: &[u8]) -> u8 {
    let mut sum = size ^ command;
    for &byte in payload {
        sum ^= byte;
    }
    sum
}

/// Encode one MSPv1 frame.
///
/// This is the low-level primitive used by the transport layer. The beeper write is only
/// ever produced through [`SetBeeperConfig::encode_request`], which cannot express an
/// arbitrary payload.
///
/// # Errors
/// [`MspError::PayloadTooLong`] if `payload` exceeds [`MAX_PAYLOAD`].
pub fn encode_frame(
    direction: Direction,
    command: CommandId,
    payload: &[u8],
) -> Result<Vec<u8>, MspError> {
    if payload.len() > MAX_PAYLOAD {
        return Err(MspError::PayloadTooLong(payload.len()));
    }
    let size = payload.len() as u8;
    let cmd = command.as_u8();
    let mut out = Vec::with_capacity(FRAME_OVERHEAD + payload.len());
    out.push(PREAMBLE_DOLLAR);
    out.push(PREAMBLE_M);
    out.push(direction.as_byte());
    out.push(size);
    out.push(cmd);
    out.extend_from_slice(payload);
    out.push(checksum(size, cmd, payload));
    Ok(out)
}

/// Decode exactly one MSPv1 frame from `bytes`, rejecting anything malformed.
///
/// # Errors
/// Any structural problem — short buffer, bad preamble/direction, bad checksum, or trailing
/// bytes after a complete frame — is returned as an [`MspError`]. Never panics.
pub fn decode_frame(bytes: &[u8]) -> Result<Frame, MspError> {
    if bytes.len() < FRAME_OVERHEAD {
        return Err(MspError::Truncated {
            needed: FRAME_OVERHEAD,
            got: bytes.len(),
        });
    }
    if bytes[0] != PREAMBLE_DOLLAR || bytes[1] != PREAMBLE_M {
        return Err(MspError::BadPreamble);
    }
    let direction = Direction::from_byte(bytes[2])?;
    let size = bytes[3] as usize;
    let command = bytes[4];
    let frame_len = FRAME_OVERHEAD + size;
    if bytes.len() < frame_len {
        return Err(MspError::Truncated {
            needed: frame_len,
            got: bytes.len(),
        });
    }
    if bytes.len() > frame_len {
        return Err(MspError::TrailingBytes {
            frame_len,
            got: bytes.len(),
        });
    }
    let payload = &bytes[5..5 + size];
    let found = bytes[5 + size];
    let expected = checksum(bytes[3], command, payload);
    if expected != found {
        return Err(MspError::BadChecksum { expected, found });
    }
    Ok(Frame {
        direction,
        command,
        payload: payload.to_vec(),
    })
}

/// A bounds-checked forward reader over a payload. Every read verifies remaining length.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    const fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], MspError> {
        if self.remaining() < n {
            return Err(MspError::FieldOverrun {
                field_len: n,
                remaining: self.remaining(),
            });
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, MspError> {
        Ok(self.take(1)?[0])
    }

    fn u16_le(&mut self) -> Result<u16, MspError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    fn u32_le(&mut self) -> Result<u32, MspError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    /// A `u8` length prefix followed by that many bytes, interpreted as UTF-8 lossily so
    /// arbitrary input never panics and no raw bytes leak through `Debug`.
    fn length_prefixed_string(&mut self) -> Result<String, MspError> {
        let len = self.u8()? as usize;
        let raw = self.take(len)?;
        Ok(String::from_utf8_lossy(raw).into_owned())
    }
}

/// `MSP_API_VERSION` reply — three bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiVersion {
    /// MSP protocol version byte.
    pub protocol_version: u8,
    /// API major.
    pub api_major: u8,
    /// API minor.
    pub api_minor: u8,
}

impl ApiVersion {
    /// Decode from a `MSP_API_VERSION` reply frame.
    ///
    /// # Errors
    /// Wrong command/direction, or a payload that is not exactly three bytes.
    pub fn from_reply(frame: &Frame) -> Result<Self, MspError> {
        frame.expect(CommandId::ApiVersion, true)?;
        if frame.payload_len() != 3 {
            return Err(MspError::WrongLength {
                expected: 3,
                got: frame.payload_len(),
            });
        }
        let p = frame.payload();
        Ok(Self {
            protocol_version: p[0],
            api_major: p[1],
            api_minor: p[2],
        })
    }
}

/// `MSP_FC_VARIANT` reply — a four-byte ASCII identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FcVariant {
    /// The four identifier bytes (e.g. `BTFL`).
    pub identifier: [u8; 4],
}

impl FcVariant {
    /// Decode from a `MSP_FC_VARIANT` reply frame.
    ///
    /// # Errors
    /// Wrong command/direction, or a payload that is not exactly four bytes.
    pub fn from_reply(frame: &Frame) -> Result<Self, MspError> {
        frame.expect(CommandId::FcVariant, true)?;
        if frame.payload_len() != 4 {
            return Err(MspError::WrongLength {
                expected: 4,
                got: frame.payload_len(),
            });
        }
        let p = frame.payload();
        Ok(Self {
            identifier: [p[0], p[1], p[2], p[3]],
        })
    }

    /// The identifier as a lossy string, for display in reports. Never panics.
    #[must_use]
    pub fn identifier_string(&self) -> String {
        String::from_utf8_lossy(&self.identifier).into_owned()
    }
}

/// `MSP_FC_VERSION` reply — major/minor/patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FcVersion {
    /// Firmware major.
    pub major: u8,
    /// Firmware minor.
    pub minor: u8,
    /// Firmware patch.
    pub patch: u8,
}

impl FcVersion {
    /// Decode from a `MSP_FC_VERSION` reply frame.
    ///
    /// # Errors
    /// Wrong command/direction, or a payload that is not exactly three bytes.
    pub fn from_reply(frame: &Frame) -> Result<Self, MspError> {
        frame.expect(CommandId::FcVersion, true)?;
        if frame.payload_len() != 3 {
            return Err(MspError::WrongLength {
                expected: 3,
                got: frame.payload_len(),
            });
        }
        let p = frame.payload();
        Ok(Self {
            major: p[0],
            minor: p[1],
            patch: p[2],
        })
    }
}

/// `MSP_BOARD_INFO` reply — variable length, parsed with bounds checks on every field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardInfo {
    /// Four-byte board identifier.
    pub board_identifier: [u8; 4],
    /// Hardware revision.
    pub hardware_revision: u16,
    /// FC type byte (0 = FC, 2 = FC with OSD chip).
    pub fc_type: u8,
    /// Target capability bitfield.
    pub target_capabilities: u8,
    /// Target name.
    pub target_name: String,
    /// Board name.
    pub board_name: String,
    /// Manufacturer identifier.
    pub manufacturer_id: String,
    /// 32-byte signature.
    pub signature: [u8; 32],
}

impl BoardInfo {
    /// Decode from a `MSP_BOARD_INFO` reply frame with a fully bounded parser.
    ///
    /// # Errors
    /// Wrong command/direction, or any length prefix that would read past the frame.
    pub fn from_reply(frame: &Frame) -> Result<Self, MspError> {
        frame.expect(CommandId::BoardInfo, true)?;
        let mut r = Reader::new(frame.payload());
        let id = r.take(4)?;
        let board_identifier = [id[0], id[1], id[2], id[3]];
        let hardware_revision = r.u16_le()?;
        let fc_type = r.u8()?;
        let target_capabilities = r.u8()?;
        let target_name = r.length_prefixed_string()?;
        let board_name = r.length_prefixed_string()?;
        let manufacturer_id = r.length_prefixed_string()?;
        let sig = r.take(32)?;
        let mut signature = [0u8; 32];
        signature.copy_from_slice(sig);
        Ok(Self {
            board_identifier,
            hardware_revision,
            fc_type,
            target_capabilities,
            target_name,
            board_name,
            manufacturer_id,
            signature,
        })
    }
}

/// `MSP_BEEPER_CONFIG` reply — the full nine-byte beeper snapshot. Every field is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeeperConfigSnapshot {
    /// Beeper disable bitmask; a set bit disables that condition.
    pub beeper_off_flags: u32,
    /// DShot beacon tone.
    pub dshot_beacon_tone: u8,
    /// DShot beacon disable bitmask.
    pub dshot_beacon_off_flags: u32,
}

impl BeeperConfigSnapshot {
    /// Decode from a `MSP_BEEPER_CONFIG` reply frame (exactly nine bytes).
    ///
    /// # Errors
    /// Wrong command/direction, or a payload that is not exactly nine bytes.
    pub fn from_reply(frame: &Frame) -> Result<Self, MspError> {
        frame.expect(CommandId::BeeperConfig, true)?;
        if frame.payload_len() != 9 {
            return Err(MspError::WrongLength {
                expected: 9,
                got: frame.payload_len(),
            });
        }
        let mut r = Reader::new(frame.payload());
        Ok(Self {
            beeper_off_flags: r.u32_le()?,
            dshot_beacon_tone: r.u8()?,
            dshot_beacon_off_flags: r.u32_le()?,
        })
    }

    /// Encode this snapshot back into the nine-byte reply payload (used by the mock FC).
    #[must_use]
    pub fn to_reply_payload(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(9);
        out.extend_from_slice(&self.beeper_off_flags.to_le_bytes());
        out.push(self.dshot_beacon_tone);
        out.extend_from_slice(&self.dshot_beacon_off_flags.to_le_bytes());
        out
    }

    /// Whether the power-on init beep is currently disabled.
    #[must_use]
    pub fn system_init_disabled(&self) -> bool {
        self.beeper_off_flags & SYSTEM_INIT_OFF_MASK != 0
    }
}

/// The M1 beeper write. By construction it can only carry a new `beeper_off_flags` value,
/// encoded as exactly four bytes, so the DShot beacon fields are never rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetBeeperConfig {
    /// The new `beeper_off_flags` value to write.
    pub beeper_off_flags: u32,
}

impl SetBeeperConfig {
    /// Build the write from a desired `beeper_off_flags` value.
    #[must_use]
    pub const fn new(beeper_off_flags: u32) -> Self {
        Self { beeper_off_flags }
    }

    /// The four-byte request payload (`beeper_off_flags` only, little-endian).
    #[must_use]
    pub fn payload(&self) -> [u8; 4] {
        self.beeper_off_flags.to_le_bytes()
    }

    /// Encode a complete `MSP_SET_BEEPER_CONFIG` request frame.
    ///
    /// # Errors
    /// Never fails in practice (four bytes is well within [`MAX_PAYLOAD`]); the [`Result`]
    /// keeps a single framing contract across the crate.
    pub fn encode_request(&self) -> Result<Vec<u8>, MspError> {
        encode_frame(
            Direction::Request,
            CommandId::SetBeeperConfig,
            &self.payload(),
        )
    }
}

/// How an incoming reply relates to the requests that were sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyClass {
    /// Matches the oldest outstanding request.
    Expected,
    /// Matches an outstanding request, but not the oldest — replies arrived out of order.
    OutOfOrder,
    /// Matches a request that was already completed — a duplicate reply.
    Duplicate,
    /// Matches no outstanding or recently completed request.
    Unsolicited,
}

/// Tracks outstanding requests so duplicate and out-of-order replies can be detected. This
/// is deliberately tiny and holds command numbers only — never payloads.
#[derive(Debug, Default, Clone)]
pub struct Correlator {
    outstanding: Vec<u8>,
    last_completed: Option<u8>,
}

impl Correlator {
    /// A fresh correlator with nothing outstanding.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a request for `command` was sent.
    pub fn on_request(&mut self, command: CommandId) {
        self.outstanding.push(command.as_u8());
    }

    /// Classify an incoming reply for `command` and update state.
    pub fn on_reply(&mut self, command: CommandId) -> ReplyClass {
        let cmd = command.as_u8();
        if self.outstanding.first() == Some(&cmd) {
            self.outstanding.remove(0);
            self.last_completed = Some(cmd);
            ReplyClass::Expected
        } else if self.outstanding.contains(&cmd) {
            ReplyClass::OutOfOrder
        } else if self.last_completed == Some(cmd) {
            ReplyClass::Duplicate
        } else {
            ReplyClass::Unsolicited
        }
    }

    /// Number of requests still awaiting a reply.
    #[must_use]
    pub fn outstanding(&self) -> usize {
        self.outstanding.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(command: CommandId, payload: &[u8]) -> Vec<u8> {
        encode_frame(Direction::Reply, command, payload).expect("payload fits")
    }

    #[test]
    fn frame_round_trip_reply() {
        let bytes = reply(CommandId::FcVersion, &[4, 5, 5]);
        let frame = decode_frame(&bytes).expect("valid frame");
        assert_eq!(frame.direction, Direction::Reply);
        assert_eq!(frame.known_command(), Some(CommandId::FcVersion));
        assert_eq!(frame.payload(), &[4, 5, 5]);
    }

    #[test]
    fn set_beeper_config_is_exactly_four_payload_bytes() {
        let set = SetBeeperConfig::new(SYSTEM_INIT_OFF_MASK);
        assert_eq!(set.payload().len(), 4);
        let bytes = set.encode_request().expect("fits");
        let frame = decode_frame(&bytes).expect("valid");
        assert_eq!(frame.direction, Direction::Request);
        assert_eq!(frame.known_command(), Some(CommandId::SetBeeperConfig));
        assert_eq!(
            frame.payload_len(),
            4,
            "the write must be exactly four bytes"
        );
        assert_eq!(frame.payload(), &SYSTEM_INIT_OFF_MASK.to_le_bytes());
    }

    #[test]
    fn beeper_snapshot_is_nine_bytes_and_round_trips() {
        let snap = BeeperConfigSnapshot {
            beeper_off_flags: 0x00AB_CD01,
            dshot_beacon_tone: 2,
            dshot_beacon_off_flags: 0x0000_0004,
        };
        let payload = snap.to_reply_payload();
        assert_eq!(payload.len(), 9, "full read is nine bytes");
        let frame = decode_frame(&reply(CommandId::BeeperConfig, &payload)).expect("valid");
        assert_eq!(
            BeeperConfigSnapshot::from_reply(&frame).expect("decodes"),
            snap
        );
    }

    #[test]
    fn system_init_mask_matches_recorded_fact() {
        assert_eq!(SYSTEM_INIT_OFF_MASK, 1 << 16);
        assert_eq!(SYSTEM_INIT_OFF_MASK, 0x0001_0000);
        let disabled = BeeperConfigSnapshot {
            beeper_off_flags: SYSTEM_INIT_OFF_MASK,
            dshot_beacon_tone: 0,
            dshot_beacon_off_flags: 0,
        };
        assert!(disabled.system_init_disabled());
        let allowed = BeeperConfigSnapshot {
            beeper_off_flags: 0,
            ..disabled
        };
        assert!(!allowed.system_init_disabled());
    }

    #[test]
    fn identity_payloads_decode() {
        let av = ApiVersion::from_reply(
            &decode_frame(&reply(CommandId::ApiVersion, &[0, 1, 46])).unwrap(),
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
        let var =
            FcVariant::from_reply(&decode_frame(&reply(CommandId::FcVariant, b"BTFL")).unwrap())
                .unwrap();
        assert_eq!(var.identifier_string(), "BTFL");
    }

    #[test]
    fn board_info_bounded_parser_round_trips() {
        let mut payload = Vec::new();
        payload.extend_from_slice(b"S405"); // board identifier
        payload.extend_from_slice(&7u16.to_le_bytes()); // hw revision
        payload.push(2); // fc type
        payload.push(0b0000_0001); // capabilities
        payload.push(13);
        payload.extend_from_slice(b"SPEEDYBEEF405");
        payload.push(4);
        payload.extend_from_slice(b"SBV4");
        payload.push(3);
        payload.extend_from_slice(b"SPB");
        payload.extend_from_slice(&[0u8; 32]); // signature
        let frame = decode_frame(&reply(CommandId::BoardInfo, &payload)).unwrap();
        let info = BoardInfo::from_reply(&frame).unwrap();
        assert_eq!(&info.board_identifier, b"S405");
        assert_eq!(info.hardware_revision, 7);
        assert_eq!(info.target_name, "SPEEDYBEEF405");
        assert_eq!(info.manufacturer_id, "SPB");
    }

    #[test]
    fn board_info_rejects_length_prefix_past_end() {
        // target_name claims 200 bytes but far fewer remain.
        let mut payload = Vec::new();
        payload.extend_from_slice(b"S405");
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.push(0);
        payload.push(0);
        payload.push(200); // lying length prefix
        payload.extend_from_slice(b"short");
        let frame = decode_frame(&reply(CommandId::BoardInfo, &payload)).unwrap();
        assert!(matches!(
            BoardInfo::from_reply(&frame),
            Err(MspError::FieldOverrun { .. })
        ));
    }

    #[test]
    fn bad_checksum_is_rejected() {
        let mut bytes = reply(CommandId::FcVersion, &[4, 5, 5]);
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(
            decode_frame(&bytes),
            Err(MspError::BadChecksum { .. })
        ));
    }

    #[test]
    fn truncation_is_rejected() {
        let bytes = reply(CommandId::BeeperConfig, &[0; 9]);
        for cut in 0..bytes.len() {
            assert!(matches!(
                decode_frame(&bytes[..cut]),
                Err(MspError::Truncated { .. })
            ));
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = reply(CommandId::FcVersion, &[4, 5, 5]);
        bytes.push(0x00);
        assert!(matches!(
            decode_frame(&bytes),
            Err(MspError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn overlong_payload_cannot_be_framed() {
        let payload = vec![0u8; MAX_PAYLOAD + 1];
        assert!(matches!(
            encode_frame(Direction::Request, CommandId::EepromWrite, &payload),
            Err(MspError::PayloadTooLong(_))
        ));
    }

    #[test]
    fn wrong_command_and_length_are_rejected() {
        let frame = decode_frame(&reply(CommandId::FcVersion, &[4, 5, 5])).unwrap();
        assert!(matches!(
            BeeperConfigSnapshot::from_reply(&frame),
            Err(MspError::WrongCommand { .. })
        ));
        let short = decode_frame(&reply(CommandId::BeeperConfig, &[0; 8])).unwrap();
        assert!(matches!(
            BeeperConfigSnapshot::from_reply(&short),
            Err(MspError::WrongLength {
                expected: 9,
                got: 8
            })
        ));
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        // Deterministic pseudo-random sweep: decode must always return, never panic.
        let mut state: u32 = 0x1234_5678;
        for len in 0..64usize {
            let mut buf = Vec::with_capacity(len);
            for _ in 0..len {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                buf.push((state >> 24) as u8);
            }
            let _ = decode_frame(&buf);
        }
    }

    #[test]
    fn correlator_flags_duplicate_and_out_of_order() {
        let mut c = Correlator::new();
        c.on_request(CommandId::ApiVersion);
        c.on_request(CommandId::FcVersion);
        assert_eq!(c.outstanding(), 2);
        // Reply to the second request first -> out of order.
        assert_eq!(c.on_reply(CommandId::FcVersion), ReplyClass::OutOfOrder);
        // Oldest reply -> expected.
        assert_eq!(c.on_reply(CommandId::ApiVersion), ReplyClass::Expected);
        // A repeat of a completed reply -> duplicate.
        assert_eq!(c.on_reply(CommandId::ApiVersion), ReplyClass::Duplicate);
        // Something never requested -> unsolicited.
        assert_eq!(c.on_reply(CommandId::EepromWrite), ReplyClass::Unsolicited);
    }

    #[test]
    fn direction_bytes_are_exact() {
        assert_eq!(Direction::Request.as_byte(), b'<');
        assert_eq!(Direction::Reply.as_byte(), b'>');
        assert_eq!(Direction::Error.as_byte(), b'!');
        assert_eq!(
            Direction::from_byte(b'?'),
            Err(MspError::BadDirection(b'?'))
        );
    }
}
