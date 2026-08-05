//! Disconnect and reconnect matching model — experimental logic only.
//!
//! The rule this exists to prove: **a COM number is not an identity.** Windows reassigns
//! COM numbers, and the same physical board can come back as a different name after a
//! reboot into bootloader, a hub change or a driver reinstall. Matching on the name alone
//! is how a tool ends up writing to the wrong device.

use std::collections::BTreeMap;

use crate::contract::PortInfo;

/// How confident we are that a port seen now is the same physical device seen before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchConfidence {
    /// Serial number matched. As strong as USB metadata gets.
    SerialNumber,
    /// VID, PID and a descriptor string matched, but no serial number was reported.
    VidPidAndDescriptor,
    /// Only VID and PID matched. Two identical boards are indistinguishable here.
    VidPidOnly,
    /// Only the port name matched, with no metadata to corroborate it.
    NameOnly,
    /// Nothing matched.
    None,
}

/// Compare a remembered device against a currently visible port.
pub fn compare(previous: &PortInfo, current: &PortInfo) -> MatchConfidence {
    let vid_pid_match = previous.vid.is_some()
        && previous.vid == current.vid
        && previous.pid.is_some()
        && previous.pid == current.pid;

    if vid_pid_match {
        if let (Some(a), Some(b)) = (&previous.serial_number, &current.serial_number) {
            if a == b {
                return MatchConfidence::SerialNumber;
            }
            // Serial numbers present and different: two different boards of one model.
            return MatchConfidence::None;
        }
        let descriptor_match = (previous.product.is_some() && previous.product == current.product)
            || (previous.manufacturer.is_some()
                && previous.manufacturer == current.manufacturer);
        if descriptor_match {
            return MatchConfidence::VidPidAndDescriptor;
        }
        return MatchConfidence::VidPidOnly;
    }

    if previous.port_name == current.port_name && previous.is_bare() && current.is_bare() {
        return MatchConfidence::NameOnly;
    }

    MatchConfidence::None
}

/// What changed between two enumeration snapshots.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Delta {
    pub appeared: Vec<String>,
    pub disappeared: Vec<String>,
    pub renamed: Vec<(String, String)>,
}

/// Diff two snapshots, reporting a rename when the same device identity appears under a
/// different port name rather than reporting it as one loss plus one arrival.
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
        // Gone by name. Did it come back under a different one?
        let moved = after
            .iter()
            .filter(|cur| !before_by_name.contains_key(cur.port_name.as_str()))
            .filter(|cur| !consumed.contains(&cur.port_name.as_str()))
            .map(|cur| (cur, compare(prev, cur)))
            .filter(|(_, c)| {
                matches!(
                    c,
                    MatchConfidence::SerialNumber | MatchConfidence::VidPidAndDescriptor
                )
            })
            .min_by_key(|(_, c)| *c);

        match moved {
            Some((cur, _)) => {
                consumed.push(cur.port_name.as_str());
                delta
                    .renamed
                    .push((prev.port_name.clone(), cur.port_name.clone()));
            }
            None => delta.disappeared.push(prev.port_name.clone()),
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
