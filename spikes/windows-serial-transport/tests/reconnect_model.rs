#![forbid(unsafe_code)]

//! Mandated identity tests. The logic is CI_VERIFIED on synthetic data; real device
//! behaviour is REQUIRES_WINDOWS_HARDWARE_TEST.

use spike_windows_serial_transport::PortInfo;
use spike_windows_serial_transport::reconnect::{
    IdentityOutcome, ReconnectResolution, WritePolicy, compare, diff, resolve,
};

fn board(name: &str, serial: Option<&str>) -> PortInfo {
    PortInfo {
        port_name: name.to_string(),
        vid: Some(0x0483),
        pid: Some(0x5740),
        manufacturer: Some("STMicroelectronics".into()),
        product: Some("Virtual COM Port".into()),
        serial_number: serial.map(str::to_string),
    }
}

// Mandated case 1: two identical boards, neither reports a serial number.
#[test]
fn identical_boards_without_serials_are_never_unique() {
    let a = board("COM3", None);
    let b = board("COM4", None);
    assert_eq!(compare(&a, &b), IdentityOutcome::PossibleMatch);
    println!("[CI_VERIFIED] identical model, no serials -> PossibleMatch, never unique");
}

// Mandated case 2: two identical boards with different serial numbers.
#[test]
fn different_serials_mean_no_match() {
    let a = board("COM3", Some("ABC123"));
    let b = board("COM4", Some("XYZ789"));
    assert_eq!(compare(&a, &b), IdentityOutcome::NoMatch);
    println!("[CI_VERIFIED] present-and-different serials -> NoMatch");
}

// Mandated case 3: one device returns on a new COM name with a matching serial.
#[test]
fn matching_serial_on_new_com_is_unique() {
    let before = board("COM3", Some("ABC123"));
    let after = board("COM7", Some("ABC123"));
    assert_eq!(
        compare(&before, &after),
        IdentityOutcome::UniqueIdentityMatch
    );
    let resolution = resolve(&before, &[after]);
    match resolution {
        ReconnectResolution::Unique {
            ref port_name,
            policy,
        } => {
            assert_eq!(port_name, "COM7");
            assert_eq!(policy, WritePolicy::BlockedUntilFirmwareHandshake);
        }
        other => panic!("expected Unique, got {other:?}"),
    }
    assert!(resolution.writes_blocked());
    println!(
        "[CI_VERIFIED] serial match on new COM -> UniqueIdentityMatch, \
         writes still blocked until firmware handshake"
    );
}

// Mandated case 4: the device disappears; a look-alike appears. Identity unproven.
#[test]
fn a_lookalike_is_possible_not_unique_and_never_auto_renamed() {
    let before = vec![board("COM3", None)];
    let after = vec![board("COM7", None)];
    let delta = diff(&before, &after);
    assert!(
        delta.renamed.is_empty(),
        "a look-alike must never auto-rename"
    );
    assert_eq!(delta.disappeared, vec!["COM3".to_string()]);
    assert_eq!(delta.appeared, vec!["COM7".to_string()]);
    assert_eq!(
        delta.possible_renames,
        vec![("COM3".to_string(), "COM7".to_string())]
    );
    println!(
        "[CI_VERIFIED] look-alike reported as disappearance + appearance with a \
         diagnostic possible_rename; no automatic rename"
    );
}

// Mandated case 5: more than one plausible candidate.
#[test]
fn multiple_candidates_are_ambiguous_and_block_writes() {
    let remembered = board("COM3", None);
    let live = vec![board("COM7", None), board("COM8", None)];
    match resolve(&remembered, &live) {
        ReconnectResolution::Ambiguous { candidates, policy } => {
            assert_eq!(candidates, vec!["COM7".to_string(), "COM8".to_string()]);
            assert_eq!(policy, WritePolicy::BlockedUntilReidentification);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
    println!("[CI_VERIFIED] two plausible candidates -> AmbiguousDeviceIdentity, writes blocked");
}

// Duplicate serial numbers across two ports (cheap clones do this).
#[test]
fn duplicate_serials_across_ports_are_ambiguous() {
    let remembered = board("COM3", Some("ABC123"));
    let live = vec![board("COM7", Some("ABC123")), board("COM8", Some("ABC123"))];
    assert!(matches!(
        resolve(&remembered, &live),
        ReconnectResolution::Ambiguous { .. }
    ));
    println!("[CI_VERIFIED] duplicated serials -> Ambiguous, not a confident unique match");
}

// Rule 4: a bare COM name is session continuity, not identity across a disappearance.
#[test]
fn a_bare_name_alone_is_not_identity_after_disappearance() {
    let before = PortInfo::named("COM3");
    let after = PortInfo::named("COM3");
    assert_eq!(compare(&before, &after), IdentityOutcome::NoMatch);
    println!("[CI_VERIFIED] bare name equality after a disappearance carries no identity");
}

#[test]
fn unrelated_device_on_a_reused_name_is_no_match() {
    let before = board("COM3", Some("ABC123"));
    let mut after = board("COM3", Some("OTHER"));
    after.vid = Some(0x1209);
    assert_eq!(compare(&before, &after), IdentityOutcome::NoMatch);
    println!("[CI_VERIFIED] reused COM name with a different device -> NoMatch");
}

#[test]
fn real_disappearance_still_reports_as_disappearance() {
    let before = vec![board("COM3", Some("ABC123"))];
    let delta = diff(&before, &[]);
    assert_eq!(delta.disappeared, vec!["COM3".to_string()]);
    assert!(delta.renamed.is_empty() && delta.possible_renames.is_empty());
    println!("[CI_VERIFIED] unplug reported as disappearance");
}

#[test]
fn no_resolution_ever_authorises_writes_from_os_metadata() {
    let remembered = board("COM3", Some("ABC123"));
    for live in [
        vec![board("COM7", Some("ABC123"))],
        vec![board("COM7", None)],
        vec![board("COM7", None), board("COM8", None)],
        vec![],
    ] {
        assert!(resolve(&remembered, &live).writes_blocked());
    }
    println!(
        "[CI_VERIFIED] every resolution keeps writes blocked; a read-only firmware \
         identity handshake is contractually required before any write (not implemented \
         in this spike)"
    );
}
