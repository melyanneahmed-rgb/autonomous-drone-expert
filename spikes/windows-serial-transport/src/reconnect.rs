//! Device identity across disconnect and reconnect — experimental logic only.
//!
//! ## The correction this module encodes
//!
//! An earlier revision of this spike treated VID/PID plus a descriptor string as enough
//! to call two sightings "the same device" and to auto-rename across a COM renumber.
//! That was wrong. **VID, PID, manufacturer and product describe a *model*, not a
//! device.** Two boards of the same model are indistinguishable by those fields, and a
//! tool that guesses writes to the wrong aircraft.
//!
//! The rules now enforced here:
//!
//! 1. Only a **non-empty, matching serial number** can yield `UniqueIdentityMatch`.
//! 2. Two present-but-different serial numbers are `NoMatch`.
//! 3. VID/PID plus manufacturer/product can yield at most `PossibleMatch` — and a
//!    possible match never produces an automatic rename.
//! 4. A COM name is **session continuity only**: it identifies a port while the device
//!    stays continuously present. Once the device disappears, the name carries no
//!    identity evidence at all.
//! 5. More than one live candidate for a remembered device is
//!    `AmbiguousDeviceIdentity`, and any future write path must stay blocked until the
//!    device is re-identified.
//!
//! ## OS metadata is never sufficient for writes
//!
//! Even `UniqueIdentityMatch` is an *operating-system* claim: it says the USB descriptor
//! matched, not that the flight controller is the one we configured. **Any future
//! reconnect that precedes a write must additionally perform a read-only identity
//! handshake with the firmware itself** — re-reading board identity over the protocol
//! and comparing it with the session's recorded identity. That handshake is a documented
//! contract for the production session layer (see `docs/TRANSPORT-CONTRACT.md`); it is
//! **not implemented in this spike**, which sends nothing to any device.

use std::collections::BTreeMap;

use crate::contract::PortInfo;

/// Identity comparison outcome between a remembered device and a live port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityOutcome {
    /// Non-empty serial numbers present on both sides and equal. The strongest claim
    /// OS metadata can make — still not sufficient for writes on its own.
    UniqueIdentityMatch,
    /// Model-level fields (VID/PID, optionally manufacturer/product) agree, but nothing
    /// proves this is the same physical unit.
    PossibleMatch,
    /// Identity cannot be resolved: more than one live candidate matches the remembered
    /// device. Writes must remain blocked until re-identification.
    AmbiguousDeviceIdentity,
    /// The evidence rules this out (or provides nothing at all).
    NoMatch,
}

/// What the session is allowed to do after a reconnect resolution.
///
/// There is deliberately **no variant that permits writes from OS metadata alone**:
/// every path requires the firmware identity handshake first, and the ambiguous path
/// requires explicit re-identification before even that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritePolicy {
    /// A single candidate was found. Writes stay blocked until the read-only firmware
    /// identity handshake confirms the board.
    BlockedUntilFirmwareHandshake,
    /// Zero or multiple candidates. Writes stay blocked until the device is explicitly
    /// re-identified (user-guided if necessary), then the handshake still applies.
    BlockedUntilReidentification,
}

/// Pairwise identity comparison. See the module rules above.
pub fn compare(previous: &PortInfo, current: &PortInfo) -> IdentityOutcome {
    let prev_serial = previous.serial_number.as_deref().filter(|s| !s.is_empty());
    let cur_serial = current.serial_number.as_deref().filter(|s| !s.is_empty());

    if let (Some(a), Some(b)) = (prev_serial, cur_serial) {
        if a == b {
            // Serial equality is only meaningful within the same model.
            let model_agrees = previous.vid == current.vid && previous.pid == current.pid;
            return if model_agrees {
                IdentityOutcome::UniqueIdentityMatch
            } else {
                IdentityOutcome::NoMatch
            };
        }
        return IdentityOutcome::NoMatch;
    }

    let vid_pid_match = previous.vid.is_some()
        && previous.vid == current.vid
        && previous.pid.is_some()
        && previous.pid == current.pid;

    if vid_pid_match {
        // Descriptor strings can strengthen the guess; they can never make it unique.
        return IdentityOutcome::PossibleMatch;
    }

    // Rule 4: a bare name match after a disappearance is not identity evidence.
    IdentityOutcome::NoMatch
}

/// Resolution of "which live port is the device I remember?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectResolution {
    /// Exactly one live port matched with `UniqueIdentityMatch`.
    Unique {
        port_name: String,
        policy: WritePolicy,
    },
    /// Exactly one live port matched, but only at `PossibleMatch` strength.
    SinglePossible {
        port_name: String,
        policy: WritePolicy,
    },
    /// More than one live port matched at or above `PossibleMatch` — or serial numbers
    /// collided across multiple ports (cheap clones do ship duplicate serials).
    Ambiguous {
        candidates: Vec<String>,
        policy: WritePolicy,
    },
    /// Nothing matched.
    NotFound { policy: WritePolicy },
}

impl ReconnectResolution {
    /// The mandated invariant: no resolution ever authorises a write by itself.
    pub fn writes_blocked(&self) -> bool {
        true
    }
}

/// Find the remembered device among live ports, honouring rule 5.
pub fn resolve(previous: &PortInfo, live: &[PortInfo]) -> ReconnectResolution {
    let mut unique: Vec<&PortInfo> = Vec::new();
    let mut possible: Vec<&PortInfo> = Vec::new();

    for port in live {
        match compare(previous, port) {
            IdentityOutcome::UniqueIdentityMatch => unique.push(port),
            IdentityOutcome::PossibleMatch => possible.push(port),
            IdentityOutcome::AmbiguousDeviceIdentity | IdentityOutcome::NoMatch => {}
        }
    }

    match (unique.len(), possible.len()) {
        (1, _) => ReconnectResolution::Unique {
            port_name: unique[0].port_name.clone(),
            policy: WritePolicy::BlockedUntilFirmwareHandshake,
        },
        (0, 1) => ReconnectResolution::SinglePossible {
            port_name: possible[0].port_name.clone(),
            policy: WritePolicy::BlockedUntilFirmwareHandshake,
        },
        (0, 0) => ReconnectResolution::NotFound {
            policy: WritePolicy::BlockedUntilReidentification,
        },
        // Duplicate serials or several look-alikes: identity is ambiguous either way.
        _ => {
            let mut candidates: Vec<String> = unique
                .iter()
                .chain(possible.iter())
                .map(|p| p.port_name.clone())
                .collect();
            candidates.sort();
            candidates.dedup();
            ReconnectResolution::Ambiguous {
                candidates,
                policy: WritePolicy::BlockedUntilReidentification,
            }
        }
    }
}

/// What changed between two enumeration snapshots.
///
/// `renamed` is populated **only** on `UniqueIdentityMatch`. A descriptor-level
/// look-alike is reported in `possible_renames` for diagnostics, and its ports still
/// appear in `appeared`/`disappeared`, because until a firmware handshake proves
/// otherwise that is what the evidence actually says.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Delta {
    pub appeared: Vec<String>,
    pub disappeared: Vec<String>,
    pub renamed: Vec<(String, String)>,
    pub possible_renames: Vec<(String, String)>,
}

/// Diff two snapshots under the identity rules.
pub fn diff(before: &[PortInfo], after: &[PortInfo]) -> Delta {
    let mut delta = Delta::default();
    let before_by_name: BTreeMap<&str, &PortInfo> =
        before.iter().map(|p| (p.port_name.as_str(), p)).collect();
    let after_by_name: BTreeMap<&str, &PortInfo> =
        after.iter().map(|p| (p.port_name.as_str(), p)).collect();

    let mut consumed: Vec<&str> = Vec::new();

    for prev in before {
        if after_by_name.contains_key(prev.port_name.as_str()) {
            continue;
        }
        let newcomers: Vec<&PortInfo> = after
            .iter()
            .filter(|cur| !before_by_name.contains_key(cur.port_name.as_str()))
            .filter(|cur| !consumed.contains(&cur.port_name.as_str()))
            .collect();

        let unique_hits: Vec<&&PortInfo> = newcomers
            .iter()
            .filter(|cur| compare(prev, cur) == IdentityOutcome::UniqueIdentityMatch)
            .collect();

        if unique_hits.len() == 1 {
            let cur = unique_hits[0];
            consumed.push(cur.port_name.as_str());
            delta
                .renamed
                .push((prev.port_name.clone(), cur.port_name.clone()));
            continue;
        }

        // No unique hit (or several — ambiguous): this is a disappearance. Record any
        // single descriptor-level look-alike as a possible rename for diagnostics only.
        delta.disappeared.push(prev.port_name.clone());
        let possible_hits: Vec<&&PortInfo> = newcomers
            .iter()
            .filter(|cur| compare(prev, cur) == IdentityOutcome::PossibleMatch)
            .collect();
        if possible_hits.len() == 1 {
            delta
                .possible_renames
                .push((prev.port_name.clone(), possible_hits[0].port_name.clone()));
        }
    }

    for cur in after {
        if before_by_name.contains_key(cur.port_name.as_str()) {
            continue;
        }
        if consumed.contains(&cur.port_name.as_str()) {
            continue;
        }
        delta.appeared.push(cur.port_name.clone());
    }

    delta
}
