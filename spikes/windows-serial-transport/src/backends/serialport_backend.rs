//! Candidate A — `serialport` 4.9.0 (MPL-2.0).

use std::time::Duration;

use crate::contract::{MetadataSupport, OpenConfig, PortInfo, SpikeTransport};
use crate::error::{Op, TransportError, classify_io_error, classify_serialport_error};

pub struct SerialportBackend {
    inner: Box<dyn serialport::SerialPort>,
}

impl SpikeTransport for SerialportBackend {
    fn backend_name() -> &'static str {
        "serialport-4.9.0"
    }

    fn metadata_support() -> MetadataSupport {
        MetadataSupport::NameAndUsbDescriptor
    }

    fn enumerate() -> Result<Vec<PortInfo>, TransportError> {
        let ports = serialport::available_ports()
            .map_err(|e| classify_serialport_error(&e, Op::Enumerate))?;
        Ok(ports
            .into_iter()
            .map(|p| {
                let mut info = PortInfo::named(p.port_name);
                if let serialport::SerialPortType::UsbPort(usb) = p.port_type {
                    info.vid = Some(usb.vid);
                    info.pid = Some(usb.pid);
                    // Every one of these is genuinely optional on Windows; missing
                    // metadata must not be an error and must not panic.
                    info.manufacturer = usb.manufacturer;
                    info.product = usb.product;
                    info.serial_number = usb.serial_number;
                }
                info
            })
            .collect())
    }

    fn open(port_name: &str, config: OpenConfig) -> Result<Self, TransportError> {
        let inner = serialport::new(port_name, config.baud)
            .timeout(config.read_timeout)
            .open()
            .map_err(|e| classify_serialport_error(&e, Op::Open))?;
        Ok(Self { inner })
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        std::io::Read::read(&mut self.inner, buf).map_err(|e| classify_io_error(&e, Op::Read))
    }

    fn write_some(&mut self, buf: &[u8]) -> Result<usize, TransportError> {
        std::io::Write::write(&mut self.inner, buf).map_err(|e| classify_io_error(&e, Op::Write))
    }

    fn flush(&mut self) -> Result<(), TransportError> {
        std::io::Write::flush(&mut self.inner).map_err(|e| classify_io_error(&e, Op::Flush))
    }

    fn close(self) -> Result<(), TransportError> {
        // The handle is released on drop. There is no explicit close in the API, which
        // matters for the cancellation story: nothing can be closed from another thread.
        drop(self.inner);
        Ok(())
    }
}

impl SerialportBackend {
    /// Read timeout as configured. `serialport` folds read and write timeouts into a
    /// single `timeout()` setting.
    pub fn configured_timeout(&self) -> Duration {
        self.inner.timeout()
    }
}
