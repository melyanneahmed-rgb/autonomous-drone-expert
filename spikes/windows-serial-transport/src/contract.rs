//! The candidate transport contract, expressed as a trait so both libraries can be
//! driven by the same harness.
//!
//! This is deliberately NOT in a production crate. It is a proposal to be judged, not an
//! API to be adopted. `docs/TRANSPORT-CONTRACT.md` explains each decision.

use std::time::Duration;

use crate::error::TransportError;

/// Device metadata as the transport layer can see it.
///
/// Every field except `port_name` is optional on purpose: a port that reports no USB
/// descriptor at all is normal, and the layer must never pretend otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortInfo {
    pub port_name: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}

impl PortInfo {
    pub fn named(name: impl Into<String>) -> Self {
        Self { port_name: name.into(), ..Default::default() }
    }

    /// True when the entry carries nothing beyond a name. Used to measure how much each
    /// library can actually tell us about a device.
    pub fn is_bare(&self) -> bool {
        self.vid.is_none()
            && self.pid.is_none()
            && self.manufacturer.is_none()
            && self.product.is_none()
            && self.serial_number.is_none()
    }
}

/// How much device metadata a backend is capable of reporting, independent of whether
/// any device is currently attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataSupport {
    /// Name plus USB descriptor fields.
    NameAndUsbDescriptor,
    /// Name only. Identity matching across a reconnect must come from elsewhere.
    NameOnly,
}

#[derive(Debug, Clone, Copy)]
pub struct OpenConfig {
    pub baud: u32,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
}

impl Default for OpenConfig {
    fn default() -> Self {
        Self {
            baud: 115_200,
            read_timeout: Duration::from_millis(250),
            write_timeout: Duration::from_millis(250),
        }
    }
}

/// The contract both candidates are driven through.
///
/// Blocking on purpose. Cancellation and reconnection are modelled above this layer
/// rather than assumed from the library, because neither candidate offers a real
/// cancel primitive (see `docs/REPORT.md`).
pub trait SpikeTransport: Sized {
    fn backend_name() -> &'static str;
    fn metadata_support() -> MetadataSupport;

    fn enumerate() -> Result<Vec<PortInfo>, TransportError>;
    fn open(port_name: &str, config: OpenConfig) -> Result<Self, TransportError>;

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;
    fn write(&mut self, buf: &[u8]) -> Result<usize, TransportError>;
    fn flush(&mut self) -> Result<(), TransportError>;
    fn close(self) -> Result<(), TransportError>;
}
