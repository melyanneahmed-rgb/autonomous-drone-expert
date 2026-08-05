//! Disconnect and reconnect matching — the logic is CI_VERIFIED, the device behaviour is
//! REQUIRES_WINDOWS_HARDWARE_TEST.

use spike_windows_serial_transport::reconnect::{compare, diff, MatchConfidence};
use spike_windows_serial_transport::PortInfo;

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

#[test]
fn same_board_on_a_new_com_number_is_recognised() {
    let before = board("COM3", Some("ABC123"));
    let after = board("COM7", Some("ABC123"));
    assert_eq!(compare(&before, &after), MatchConfidence::SerialNumber);
    println!("[CI_VERIFIED] COM3 -> COM7 matched by serial number");
}

#[test]
fn two_identical_boards_are_not_confused_when_serials_exist() {
    let a = board("COM3", Some("ABC123"));
    let b = board("COM4", Some("XYZ789"));
    assert_eq!(compare(&a, &b), MatchConfidence::None);
    println!("[CI_VERIFIED] different serial numbers are never treated as the same device");
}

#[test]
fn without_a_serial_number_confidence_degrades_honestly() {
    let a = board("COM3", None);
    let b = board("COM7", None);
    assert_eq!(compare(&a, &b), MatchConfidence::VidPidAndDescriptor);
    let bare_a = PortInfo::named("COM3");
    let bare_b = PortInfo::named("COM3");
    assert_eq!(compare(&bare_a, &bare_b), MatchConfidence::NameOnly);
    println!("[CI_VERIFIED] confidence degrades to descriptor, then to name-only");
}

#[test]
fn name_alone_is_never_treated_as_identity_when_metadata_exists() {
    // Same COM number, different device: must not match.
    let before = board("COM3", Some("ABC123"));
    let mut after = board("COM3", Some("ABC123"));
    after.vid = Some(0x1209);
    after.serial_number = Some("OTHER".into());
    assert_eq!(compare(&before, &after), MatchConfidence::None);
    println!("[CI_VERIFIED] a reused COM number with a different device does not match");
}

#[test]
fn a_replug_under_a_new_name_reads_as_a_rename_not_a_loss() {
    let before = vec![board("COM3", Some("ABC123")), PortInfo::named("COM1")];
    let after = vec![board("COM7", Some("ABC123")), PortInfo::named("COM1")];
    let delta = diff(&before, &after);
    assert_eq!(delta.renamed, vec![("COM3".to_string(), "COM7".to_string())]);
    assert!(delta.appeared.is_empty() && delta.disappeared.is_empty());
    println!("[CI_VERIFIED] replug on a new COM number reported as a rename");
}

#[test]
fn a_real_disappearance_is_reported_as_a_disappearance() {
    let before = vec![board("COM3", Some("ABC123"))];
    let after: Vec<PortInfo> = vec![];
    let delta = diff(&before, &after);
    assert_eq!(delta.disappeared, vec!["COM3".to_string()]);
    assert!(delta.renamed.is_empty());
    println!("[CI_VERIFIED] unplug reported as disappearance, not as a rename");
}

#[test]
fn name_only_backends_cannot_survive_a_com_renumber() {
    // This is the practical consequence of serial2 reporting names only: after a COM
    // renumber there is nothing left to match on.
    let before = vec![PortInfo::named("COM3")];
    let after = vec![PortInfo::named("COM7")];
    let delta = diff(&before, &after);
    assert_eq!(delta.disappeared, vec!["COM3".to_string()]);
    assert_eq!(delta.appeared, vec!["COM7".to_string()]);
    assert!(delta.renamed.is_empty());
    println!("[CI_VERIFIED] without metadata a renumber is indistinguishable from unplug+plug");
}
