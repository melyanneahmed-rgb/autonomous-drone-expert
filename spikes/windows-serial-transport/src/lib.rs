#![forbid(unsafe_code)]

//! # M1A — Windows serial transport spike
//!
//! **This is not production code and must never be merged into `main`.** It exists to
//! answer one question with evidence instead of opinion: which Rust serial layer should
//! the production transport crate be built on for Windows first?
//!
//! `#![forbid(unsafe_code)]` is kept deliberately. If either candidate forced `unsafe`
//! into our own code, this crate would not compile — and that would itself be a finding.
//!
//! Read `docs/REPORT.md` for the comparison and `docs/TRANSPORT-CONTRACT.md` for the
//! proposed contract. Nothing here is adopted until the owner approves it.

pub mod backends;
pub mod contract;
pub mod error;
pub mod reconnect;
pub mod watchdog;

pub use contract::{MetadataSupport, OpenConfig, PortInfo, SpikeTransport};
pub use error::{Op, TransportError};

/// Port names that cannot exist on a runner, used to exercise the failure paths that CI
/// *can* reach without hardware.
pub mod probes {
    /// Syntactically valid Windows COM name, almost certainly absent.
    pub const ABSENT_COM: &str = "COM231";
    /// Above COM9, so it requires the `\\.\` device-namespace prefix to open at all.
    /// Both candidates were confirmed to apply that prefix.
    pub const ABSENT_COM_HIGH: &str = "COM188";
    /// Not a port name in any sense.
    pub const INVALID_NAME: &str = "definitely-not-a-serial-port";
    /// Empty name.
    pub const EMPTY_NAME: &str = "";
}
