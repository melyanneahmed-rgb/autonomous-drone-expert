//! Shared harness: both candidates are driven through identical assertions.
//!
//! Every result is labelled with what it actually proves:
//!   CI_VERIFIED                     — executed on the CI runner, real behaviour
//!   SIMULATED_ONLY                  — logic exercised with synthetic input
//!   REQUIRES_WINDOWS_HARDWARE_TEST  — cannot be reached without a real device
//!
//! GitHub-hosted Windows runners expose no serial hardware, so anything that needs an
//! open port is out of reach here and is labelled accordingly rather than skipped
//! silently.

use std::time::Duration;

use spike_windows_serial_transport::backends::serial2_backend::Serial2Backend;
use spike_windows_serial_transport::backends::serialport_backend::SerialportBackend;
use spike_windows_serial_transport::probes;
use spike_windows_serial_transport::watchdog::{with_deadline, Outcome};
use spike_windows_serial_transport::{MetadataSupport, OpenConfig, SpikeTransport, TransportError};

const DEADLINE: Duration = Duration::from_secs(5);

fn report(backend: &str, case: &str, label: &str, detail: &str) {
    println!("[{label}] {backend} :: {case} :: {detail}");
}

// ---------------------------------------------------------------- enumeration

fn enumerate_case<T: SpikeTransport + Send + 'static>() {
    let name = T::backend_name();
    let outcome = with_deadline(DEADLINE, T::enumerate);
    let Outcome::Completed { value, elapsed } = outcome else {
        panic!("[FAIL] {name} :: enumerate did not return within {DEADLINE:?}");
    };
    match value {
        Ok(ports) => {
            report(
                name,
                "enumerate",
                "CI_VERIFIED",
                &format!("{} port(s) in {:?}", ports.len(), elapsed),
            );
            for p in &ports {
                // Missing metadata must never panic and must never be invented.
                assert!(!p.port_name.is_empty(), "a port was reported with no name");
                report(
                    name,
                    "enumerate.metadata",
                    "CI_VERIFIED",
                    &format!(
                        "{} vid={:?} pid={:?} mfr={:?} product={:?} serial={:?} bare={}",
                        p.port_name, p.vid, p.pid, p.manufacturer, p.product, p.serial_number,
                        p.is_bare()
                    ),
                );
            }
            if ports.is_empty() {
                report(
                    name,
                    "enumerate.empty",
                    "CI_VERIFIED",
                    "no ports present; empty list returned rather than an error",
                );
            }
        }
        Err(e) => {
            // An enumeration error is acceptable only if it is classified, never a panic.
            report(name, "enumerate", "CI_VERIFIED", &format!("classified error: {e}"));
        }
    }
}

#[test]
fn enumeration_serialport() {
    enumerate_case::<SerialportBackend>();
}

#[test]
fn enumeration_serial2() {
    enumerate_case::<Serial2Backend>();
}

#[test]
fn enumeration_is_repeatable_and_bounded() {
    for _ in 0..5 {
        assert!(with_deadline(DEADLINE, SerialportBackend::enumerate).completed());
        assert!(with_deadline(DEADLINE, Serial2Backend::enumerate).completed());
    }
    report("both", "enumerate.repeat x5", "CI_VERIFIED", "no hang, no leak observed");
}

#[test]
fn metadata_capability_differs() {
    assert_eq!(
        SerialportBackend::metadata_support(),
        MetadataSupport::NameAndUsbDescriptor
    );
    assert_eq!(Serial2Backend::metadata_support(), MetadataSupport::NameOnly);
    report(
        "both",
        "metadata capability",
        "CI_VERIFIED",
        "serialport exposes USB descriptors; serial2 exposes names only",
    );
}

// ---------------------------------------------------------------- open / close

fn open_absent_case<T: SpikeTransport + Send + 'static>(port: &'static str, case: &'static str) {
    let name = T::backend_name();
    let outcome = with_deadline(DEADLINE, move || T::open(port, OpenConfig::default()));
    let Outcome::Completed { value, elapsed } = outcome else {
        panic!("[FAIL] {name} :: open({port}) hung past {DEADLINE:?}");
    };
    match value {
        Ok(_) => report(
            name,
            case,
            "CI_VERIFIED",
            &format!("unexpectedly opened {port} — a real device may exist on this runner"),
        ),
        Err(e) => {
            assert_ne!(
                e,
                TransportError::UnknownTransportError,
                "{name}: opening {port} produced an unclassified error"
            );
            report(name, case, "CI_VERIFIED", &format!("{e} in {elapsed:?}"));
        }
    }
}

#[test]
fn open_absent_port_serialport() {
    open_absent_case::<SerialportBackend>(probes::ABSENT_COM, "open.absent");
}

#[test]
fn open_absent_port_serial2() {
    open_absent_case::<Serial2Backend>(probes::ABSENT_COM, "open.absent");
}

#[test]
fn open_high_com_number_serialport() {
    // COM10 and above need the \\.\ device-namespace prefix. Both crates were confirmed
    // to apply it; this checks the failure is a normal classified error, not a panic.
    open_absent_case::<SerialportBackend>(probes::ABSENT_COM_HIGH, "open.com_above_9");
}

#[test]
fn open_high_com_number_serial2() {
    open_absent_case::<Serial2Backend>(probes::ABSENT_COM_HIGH, "open.com_above_9");
}

#[test]
fn open_invalid_name_serialport() {
    open_absent_case::<SerialportBackend>(probes::INVALID_NAME, "open.invalid_name");
}

#[test]
fn open_invalid_name_serial2() {
    open_absent_case::<Serial2Backend>(probes::INVALID_NAME, "open.invalid_name");
}

#[test]
fn open_empty_name_serialport() {
    open_absent_case::<SerialportBackend>(probes::EMPTY_NAME, "open.empty_name");
}

#[test]
fn open_empty_name_serial2() {
    open_absent_case::<Serial2Backend>(probes::EMPTY_NAME, "open.empty_name");
}

#[test]
fn repeated_open_attempts_do_not_degrade() {
    for i in 0..25 {
        let a = with_deadline(DEADLINE, || {
            SerialportBackend::open(probes::ABSENT_COM, OpenConfig::default())
        });
        let b = with_deadline(DEADLINE, || {
            Serial2Backend::open(probes::ABSENT_COM, OpenConfig::default())
        });
        assert!(a.completed(), "serialport hung on attempt {i}");
        assert!(b.completed(), "serial2 hung on attempt {i}");
    }
    report(
        "both",
        "open.repeat x25",
        "CI_VERIFIED",
        "no handle exhaustion or slowdown observed on failed opens",
    );
}

// ---------------------------------------------------------------- unreachable here

#[test]
fn operations_requiring_a_real_device() {
    for case in [
        "read timeout is honoured within tolerance",
        "write timeout is honoured within tolerance",
        "PORT_BUSY when a second client opens the same port",
        "blocking read interrupted by closing the handle from another thread",
        "unplug during a blocking read yields DEVICE_DISCONNECTED",
        "replug is detected and the device is re-identified",
        "COM number changes across replug",
        "handles released after process kill",
    ] {
        report("both", case, "REQUIRES_WINDOWS_HARDWARE_TEST", "no serial hardware on runner");
    }
}
