#![forbid(unsafe_code)]

//! Path-C discovery probe and architecture-B wiring — labelled honestly.

use std::time::Duration;

use spike_windows_serial_transport::backends::serial2_backend::Serial2Backend;
use spike_windows_serial_transport::backends::serialport_backend::SerialportBackend;
use spike_windows_serial_transport::discovery;
use spike_windows_serial_transport::probes;
use spike_windows_serial_transport::watchdog::{Outcome, with_deadline};
use spike_windows_serial_transport::{OpenConfig, SpikeTransport, TransportError};

const DEADLINE: Duration = Duration::from_secs(10);

/// Path C, nusb leg: enumeration must return without hanging or panicking, and every
/// exposed field must tolerate absence.
#[test]
fn nusb_usb_enumeration_is_bounded_and_tolerant() {
    let outcome = with_deadline(DEADLINE, discovery::probe_usb_devices);
    let Outcome::Completed { value, elapsed } = outcome else {
        panic!("nusb enumeration hung past {DEADLINE:?}");
    };
    match value {
        Ok(devices) => {
            println!(
                "[CI_VERIFIED] nusb-0.2.5 :: usb enumerate :: {} device(s) in {elapsed:?}",
                devices.len()
            );
            for d in &devices {
                println!(
                    "[CI_VERIFIED] nusb device vid={:04x} pid={:04x} serial={:?} mfr={:?} \
                     product={:?} instance_id={:?}",
                    d.vid, d.pid, d.serial_number, d.manufacturer, d.product, d.instance_id
                );
            }
            if devices.is_empty() {
                println!(
                    "[CI_VERIFIED] nusb :: zero USB devices on this runner; empty list, no error"
                );
            }
        }
        Err(e) => {
            // A classified failure is acceptable on a virtualised runner; a hang or
            // panic is not.
            println!("[CI_VERIFIED] nusb :: enumeration returned a clean error: {e}");
        }
    }
}

/// The finding the report cites as an executable statement, not prose.
#[test]
fn nusb_cannot_produce_a_com_name_by_itself() {
    assert!(!discovery::can_map_usb_identity_to_com_name());
    println!(
        "[CI_VERIFIED] nusb exposes the Windows instance_id join key but no COM name; \
         the SetupAPI/registry join is unimplemented here and its end-to-end behaviour \
         is REQUIRES_WINDOWS_HARDWARE_TEST"
    );
}

/// Architecture B wiring: discovery through `serialport`, I/O through `serial2`.
/// Proves the hybrid composes and both halves fail classified; real traffic through the
/// pair needs hardware.
#[test]
fn architecture_b_wiring_composes() {
    let enumerated = with_deadline(DEADLINE, SerialportBackend::enumerate);
    assert!(enumerated.completed(), "serialport enumeration hung");

    let opened = with_deadline(DEADLINE, || {
        Serial2Backend::open(probes::ABSENT_COM, OpenConfig::default())
    });
    let Outcome::Completed { value, .. } = opened else {
        panic!("serial2 open hung");
    };
    match value {
        Ok(_) => println!(
            "[CI_VERIFIED] architecture B :: unexpectedly opened {} — a real device may exist",
            probes::ABSENT_COM
        ),
        Err(e) => {
            assert_ne!(e, TransportError::UnknownTransportError);
            println!(
                "[CI_VERIFIED] architecture B :: serialport enumerates + serial2 open \
                 fails classified ({e}); end-to-end I/O through the hybrid is \
                 REQUIRES_WINDOWS_HARDWARE_TEST"
            );
        }
    }
}
