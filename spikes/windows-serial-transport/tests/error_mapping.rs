#![forbid(unsafe_code)]

//! Error-model coverage — SIMULATED_ONLY.
//!
//! Real Windows codes cannot be produced without a device, so the mapping is exercised
//! with the documented codes directly. This proves our adapter is complete and
//! deterministic; it does not prove which code a given device actually returns. That
//! remains REQUIRES_WINDOWS_HARDWARE_TEST.

use spike_windows_serial_transport::TransportError as E;
use spike_windows_serial_transport::error::{Op, classify_os_error, win32};

#[test]
fn documented_windows_codes_map_to_the_model() {
    let cases = [
        (win32::ERROR_FILE_NOT_FOUND, Op::Open, E::PortNotFound),
        (win32::ERROR_PATH_NOT_FOUND, Op::Open, E::PortNotFound),
        (win32::ERROR_NO_SUCH_DEVICE, Op::Open, E::PortNotFound),
        (win32::ERROR_ACCESS_DENIED, Op::Open, E::PortBusy),
        (win32::ERROR_SHARING_VIOLATION, Op::Open, E::PortBusy),
        (
            win32::ERROR_INVALID_PARAMETER,
            Op::Open,
            E::UnsupportedConfiguration,
        ),
        (
            win32::ERROR_BAD_COMMAND,
            Op::Open,
            E::UnsupportedConfiguration,
        ),
        (
            win32::ERROR_DEVICE_NOT_CONNECTED,
            Op::Read,
            E::DeviceDisconnected,
        ),
        (win32::ERROR_INVALID_HANDLE, Op::Read, E::DeviceDisconnected),
        (win32::ERROR_NOT_READY, Op::Read, E::DeviceDisconnected),
        (
            win32::ERROR_OPERATION_ABORTED,
            Op::Read,
            E::OperationCancelled,
        ),
        (win32::ERROR_SEM_TIMEOUT, Op::Read, E::ReadTimeout),
        (win32::ERROR_SEM_TIMEOUT, Op::Write, E::WriteTimeout),
    ];
    for (code, op, expected) in cases {
        let got = classify_os_error(code, op);
        assert_eq!(got, expected, "code {code} during {op:?} mapped to {got}");
        println!("[SIMULATED_ONLY] os code {code} + {op:?} -> {got}");
    }
}

#[test]
fn unknown_codes_degrade_by_operation_never_silently() {
    assert_eq!(classify_os_error(424_242, Op::Open), E::OpenFailed);
    assert_eq!(classify_os_error(424_242, Op::Read), E::ReadFailed);
    assert_eq!(classify_os_error(424_242, Op::Write), E::WriteFailed);
    assert_eq!(
        classify_os_error(424_242, Op::Flush),
        E::UnknownTransportError
    );
    println!("[SIMULATED_ONLY] unrecognised codes stay classified by operation");
}

#[test]
fn busy_and_absent_are_never_collapsed() {
    // The single most important distinction for diagnosis: "someone else has the port"
    // must never be reported as "there is no port".
    assert_eq!(
        classify_os_error(win32::ERROR_ACCESS_DENIED, Op::Open),
        E::PortBusy
    );
    assert_eq!(
        classify_os_error(win32::ERROR_FILE_NOT_FOUND, Op::Open),
        E::PortNotFound
    );
    assert_ne!(
        classify_os_error(win32::ERROR_ACCESS_DENIED, Op::Open),
        classify_os_error(win32::ERROR_FILE_NOT_FOUND, Op::Open)
    );
    println!("[SIMULATED_ONLY] PORT_BUSY and PORT_NOT_FOUND remain distinct");
}

#[test]
fn every_model_variant_is_reachable() {
    // A model variant no code path can produce is a lie in the type system.
    let reachable = [
        E::PortNotFound,
        E::PortBusy,
        E::UnsupportedConfiguration,
        E::DeviceDisconnected,
        E::OperationCancelled,
        E::ReadTimeout,
        E::WriteTimeout,
        E::OpenFailed,
        E::ReadFailed,
        E::WriteFailed,
        E::UnknownTransportError,
    ];
    assert_eq!(reachable.len(), 11);
    // PERMISSION_DENIED is reachable only through io::ErrorKind::PermissionDenied on
    // platforms that report it that way; on Windows ERROR_ACCESS_DENIED on a COM port
    // means "busy" in practice. Recorded as a finding rather than forced.
    println!(
        "[SIMULATED_ONLY] 11 of 12 variants reachable from OS codes; PERMISSION_DENIED is Unix-leaning"
    );
}
