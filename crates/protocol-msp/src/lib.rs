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
    /// A payload had bytes left over after every documented field was read. The M1 codec
    /// pins API 1.46 and refuses to silently ignore an undocumented tail.
    TrailingPayload { consumed: usize, total: usize },
    /// A text field that feeds the device identity was not valid UTF-8. Rejected outright:
    /// a lossy conversion could collapse two different byte sequences into the same string
    /// and let two different devices compare as the same identity.
    InvalidUtf8 { field: &'static str },
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

    /// A `u8` length prefix followed by that many bytes, decoded as **strict** UTF-8.
    ///
    /// These strings feed the device identity, so a lossy decode is forbidden: it would map
    /// distinct invalid byte sequences onto the same replacement-character string and two
    /// different devices could then compare as the same identity. Invalid UTF-8 is rejected
    /// with [`MspError::InvalidUtf8`] naming the field (never echoing the bytes).
    fn length_prefixed_string(&mut self, field: &'static str) -> Result<String, MspError> {
        let len = self.u8()? as usize;
        let raw = self.take(len)?;
        match std::str::from_utf8(raw) {
            Ok(text) => Ok(text.to_owned()),
            Err(_) => Err(MspError::InvalidUtf8 { field }),
        }
    }

    /// Assert the payload was fully consumed, rejecting any undocumented trailing bytes.
    fn finish(&self) -> Result<(), MspError> {
        if self.remaining() != 0 {
            return Err(MspError::TrailingPayload {
                consumed: self.pos,
                total: self.bytes.len(),
            });
        }
        Ok(())
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

    /// The identifier as a lossy string — a **presentation helper only**, for human-facing
    /// reports. Identity comparison never uses this: it compares the raw `identifier` bytes,
    /// so distinct byte sequences can never collapse into the same identity here.
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

/// Number of signature bytes in a `MSP_BOARD_INFO` reply (Betaflight `SIGNATURE_LENGTH`).
pub const SIGNATURE_LENGTH: usize = 32;

/// `MSP_BOARD_INFO` reply — variable length, parsed with bounds checks on every field.
///
/// M1 pins MSP API 1.46, so this parser reads the **complete** payload: the identity block,
/// the three length-prefixed names, the 32-byte signature, and every field appended up to
/// API 1.44 (`mcu_type_id`, `configuration_state`, `gyro_sample_rate_hz`,
/// `configuration_problems`, and the SPI/I2C device counts). A short payload is rejected as
/// truncation and any leftover byte is rejected as an undocumented trailing tail — the
/// parser never silently ignores a tail.
///
/// The `signature` is parsed so the payload can be fully consumed, but it is a per-unit
/// value: it is deliberately **not** propagated into any reconnection identity, backup or
/// case record (see `ade-facts`), and the manual [`core::fmt::Debug`] implementation redacts
/// it so the bytes can never leak through logs or assertion messages.
#[derive(Clone, PartialEq, Eq)]
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
    /// 32-byte signature (per-unit; never used as a stable identity).
    pub signature: [u8; SIGNATURE_LENGTH],
    /// MCU type identifier (API 1.35+).
    pub mcu_type_id: u8,
    /// Configuration state (API 1.42) — volatile; not part of the reconnection identity.
    pub configuration_state: u8,
    /// Gyro sample rate in Hz (API 1.43) — informational/volatile.
    pub gyro_sample_rate_hz: u16,
    /// Configuration problems bitfield (API 1.43) — volatile runtime state.
    pub configuration_problems: u32,
    /// Registered SPI device count (API 1.44) — volatile runtime state.
    pub spi_device_count: u8,
    /// Registered I2C device count (API 1.44) — volatile runtime state.
    pub i2c_device_count: u8,
}

/// Manual `Debug`: every field except the per-unit `signature`, which is redacted. No byte,
/// hex, base64 or hash representation of the signature is ever formatted.
impl core::fmt::Debug for BoardInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BoardInfo")
            .field("board_identifier", &self.board_identifier)
            .field("hardware_revision", &self.hardware_revision)
            .field("fc_type", &self.fc_type)
            .field("target_capabilities", &self.target_capabilities)
            .field("target_name", &self.target_name)
            .field("board_name", &self.board_name)
            .field("manufacturer_id", &self.manufacturer_id)
            .field("signature", &"<redacted>")
            .field("mcu_type_id", &self.mcu_type_id)
            .field("configuration_state", &self.configuration_state)
            .field("gyro_sample_rate_hz", &self.gyro_sample_rate_hz)
            .field("configuration_problems", &self.configuration_problems)
            .field("spi_device_count", &self.spi_device_count)
            .field("i2c_device_count", &self.i2c_device_count)
            .finish()
    }
}

impl BoardInfo {
    /// Decode from a `MSP_BOARD_INFO` reply frame with a fully bounded parser that consumes
    /// the entire API 1.46 payload.
    ///
    /// # Errors
    /// - wrong command/direction;
    /// - [`MspError::FieldOverrun`] for any length prefix or fixed field that would read past
    ///   the frame (truncation);
    /// - [`MspError::TrailingPayload`] if any byte remains after the last documented field;
    /// - [`MspError::InvalidUtf8`] if a name field is not valid UTF-8 (these strings feed the
    ///   device identity and are never decoded lossily).
    pub fn from_reply(frame: &Frame) -> Result<Self, MspError> {
        frame.expect(CommandId::BoardInfo, true)?;
        let mut r = Reader::new(frame.payload());
        let id = r.take(4)?;
        let board_identifier = [id[0], id[1], id[2], id[3]];
        let hardware_revision = r.u16_le()?;
        let fc_type = r.u8()?;
        let target_capabilities = r.u8()?;
        let target_name = r.length_prefixed_string("target_name")?;
        let board_name = r.length_prefixed_string("board_name")?;
        let manufacturer_id = r.length_prefixed_string("manufacturer_id")?;
        let sig = r.take(SIGNATURE_LENGTH)?;
        let mut signature = [0u8; SIGNATURE_LENGTH];
        signature.copy_from_slice(sig);
        let mcu_type_id = r.u8()?;
        let configuration_state = r.u8()?;
        let gyro_sample_rate_hz = r.u16_le()?;
        let configuration_problems = r.u32_le()?;
        let spi_device_count = r.u8()?;
        let i2c_device_count = r.u8()?;
        r.finish()?;
        Ok(Self {
            board_identifier,
            hardware_revision,
            fc_type,
            target_capabilities,
            target_name,
            board_name,
            manufacturer_id,
            signature,
            mcu_type_id,
            configuration_state,
            gyro_sample_rate_hz,
            configuration_problems,
            spi_device_count,
            i2c_device_count,
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

    /// Build a complete API 1.46 `MSP_BOARD_INFO` payload for tests.
    #[allow(clippy::too_many_arguments)]
    fn board_info_payload(
        id: &[u8; 4],
        hw_rev: u16,
        fc_type: u8,
        caps: u8,
        target: &[u8],
        board: &[u8],
        mfr: &[u8],
        signature: &[u8; SIGNATURE_LENGTH],
        mcu_type_id: u8,
        configuration_state: u8,
        gyro_sample_rate_hz: u16,
        configuration_problems: u32,
        spi: u8,
        i2c: u8,
    ) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(id);
        p.extend_from_slice(&hw_rev.to_le_bytes());
        p.push(fc_type);
        p.push(caps);
        p.push(target.len() as u8);
        p.extend_from_slice(target);
        p.push(board.len() as u8);
        p.extend_from_slice(board);
        p.push(mfr.len() as u8);
        p.extend_from_slice(mfr);
        p.extend_from_slice(signature);
        p.push(mcu_type_id);
        p.push(configuration_state);
        p.extend_from_slice(&gyro_sample_rate_hz.to_le_bytes());
        p.extend_from_slice(&configuration_problems.to_le_bytes());
        p.push(spi);
        p.push(i2c);
        p
    }

    #[test]
    fn board_info_bounded_parser_round_trips_the_full_api_1_46_payload() {
        let sig = {
            let mut s = [0u8; SIGNATURE_LENGTH];
            for (i, b) in s.iter_mut().enumerate() {
                *b = i as u8;
            }
            s
        };
        let payload = board_info_payload(
            b"S405",
            7,
            2,
            0b0000_0001,
            b"SPEEDYBEEF405",
            b"SBV4",
            b"SPB",
            &sig,
            0x1B, // mcu_type_id
            3,    // configuration_state
            8000, // gyro_sample_rate_hz
            0x0000_0002,
            4, // spi count
            1, // i2c count
        );
        let frame = decode_frame(&reply(CommandId::BoardInfo, &payload)).unwrap();
        let info = BoardInfo::from_reply(&frame).unwrap();
        assert_eq!(&info.board_identifier, b"S405");
        assert_eq!(info.hardware_revision, 7);
        assert_eq!(info.target_name, "SPEEDYBEEF405");
        assert_eq!(info.manufacturer_id, "SPB");
        assert_eq!(info.signature, sig);
        assert_eq!(info.mcu_type_id, 0x1B);
        assert_eq!(info.configuration_state, 3);
        assert_eq!(info.gyro_sample_rate_hz, 8000);
        assert_eq!(info.configuration_problems, 0x0000_0002);
        assert_eq!(info.spi_device_count, 4);
        assert_eq!(info.i2c_device_count, 1);
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
    fn board_info_rejects_a_payload_truncated_after_the_signature() {
        // A payload that stops right at the signature (the pre-1.46 shape) is now truncation:
        // the API 1.46 post-signature fields are mandatory and must be present.
        let mut payload = Vec::new();
        payload.extend_from_slice(b"S405");
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.push(0);
        payload.push(0);
        payload.push(4);
        payload.extend_from_slice(b"TGT4");
        payload.push(4);
        payload.extend_from_slice(b"BRD4");
        payload.push(3);
        payload.extend_from_slice(b"SPB");
        payload.extend_from_slice(&[0u8; SIGNATURE_LENGTH]); // signature, then nothing
        let frame = decode_frame(&reply(CommandId::BoardInfo, &payload)).unwrap();
        assert!(matches!(
            BoardInfo::from_reply(&frame),
            Err(MspError::FieldOverrun { .. })
        ));
    }

    #[test]
    fn board_info_rejects_invalid_utf8_in_each_identity_string() {
        // 0xFF is never valid UTF-8. Each name field is corrupted in turn.
        let bad = [0xFFu8, 0x41];
        let sig = [0u8; SIGNATURE_LENGTH];
        let cases: [(&str, Vec<u8>); 3] = [
            (
                "target_name",
                board_info_payload(
                    b"S405", 0, 0, 0, &bad, b"BRD4", b"SPB", &sig, 0, 0, 0, 0, 0, 0,
                ),
            ),
            (
                "board_name",
                board_info_payload(
                    b"S405", 0, 0, 0, b"TGT4", &bad, b"SPB", &sig, 0, 0, 0, 0, 0, 0,
                ),
            ),
            (
                "manufacturer_id",
                board_info_payload(
                    b"S405", 0, 0, 0, b"TGT4", b"BRD4", &bad, &sig, 0, 0, 0, 0, 0, 0,
                ),
            ),
        ];
        for (field, payload) in cases {
            let frame = decode_frame(&reply(CommandId::BoardInfo, &payload)).unwrap();
            assert_eq!(
                BoardInfo::from_reply(&frame),
                Err(MspError::InvalidUtf8 { field }),
                "invalid UTF-8 in {field} must be rejected",
            );
        }
    }

    #[test]
    fn two_different_invalid_inputs_cannot_collapse_into_the_same_identity() {
        // Under a lossy decode both of these distinct target_name byte sequences would
        // become the same replacement-character string. Under the strict decode neither
        // produces a BoardInfo at all, so no identity — let alone a shared one — can exist.
        let sig = [0u8; SIGNATURE_LENGTH];
        let first = board_info_payload(
            b"S405",
            0,
            0,
            0,
            &[0xC3, 0x28],
            b"BRD4",
            b"SPB",
            &sig,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        let second = board_info_payload(
            b"S405",
            0,
            0,
            0,
            &[0xE2, 0x28],
            b"BRD4",
            b"SPB",
            &sig,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        let a = BoardInfo::from_reply(&decode_frame(&reply(CommandId::BoardInfo, &first)).unwrap());
        let b =
            BoardInfo::from_reply(&decode_frame(&reply(CommandId::BoardInfo, &second)).unwrap());
        assert_eq!(
            a,
            Err(MspError::InvalidUtf8 {
                field: "target_name"
            })
        );
        assert_eq!(
            b,
            Err(MspError::InvalidUtf8 {
                field: "target_name"
            })
        );
    }

    #[test]
    fn board_info_debug_redacts_the_signature() {
        // A non-zero signature filled with a byte value that appears nowhere else.
        let sig = [0xEEu8; SIGNATURE_LENGTH];
        let payload = board_info_payload(
            b"S405", 7, 2, 1, b"TGT4", b"BRD4", b"SPB", &sig, 3, 4, 5, 6, 7, 8,
        );
        let frame = decode_frame(&reply(CommandId::BoardInfo, &payload)).unwrap();
        let board = BoardInfo::from_reply(&frame).unwrap();
        let formatted = format!("{board:?}");
        assert!(
            formatted.contains("<redacted>"),
            "Debug must carry the redaction marker",
        );
        // 0xEE is 238 decimal; neither the decimal nor a hex spelling may appear.
        assert!(!formatted.contains("238"), "no signature byte in Debug");
        assert!(
            !formatted.to_lowercase().contains("0xee"),
            "no hex signature byte in Debug"
        );
        assert!(!formatted.contains("ee, "), "no hex byte run in Debug");
        // The other fields are still present.
        assert!(formatted.contains("TGT4"));
        assert!(formatted.contains("board_identifier"));
    }

    #[test]
    fn board_info_rejects_an_undocumented_trailing_tail() {
        let sig = [0u8; SIGNATURE_LENGTH];
        let mut payload = board_info_payload(
            b"S405", 0, 0, 0, b"TGT4", b"BRD4", b"SPB", &sig, 0, 0, 0, 0, 0, 0,
        );
        payload.push(0xAB); // one undocumented trailing byte
        let frame = decode_frame(&reply(CommandId::BoardInfo, &payload)).unwrap();
        assert!(matches!(
            BoardInfo::from_reply(&frame),
            Err(MspError::TrailingPayload {
                consumed: _,
                total: _
            })
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
