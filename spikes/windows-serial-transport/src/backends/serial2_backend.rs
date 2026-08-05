//! Candidate B — `serial2` 0.2.38 (BSD-2-Clause OR Apache-2.0).

use serial2::SerialPort;

use crate::contract::{MetadataSupport, OpenConfig, PortInfo, SpikeTransport};
use crate::error::{classify_io_error, Op, TransportError};

pub struct Serial2Backend {
    inner: SerialPort,
}

impl SpikeTransport for Serial2Backend {
    fn backend_name() -> &'static str {
        "serial2-0.2.38"
    }

    fn metadata_support() -> MetadataSupport {
        // Decisive limitation: available_ports() yields paths only.
        MetadataSupport::NameOnly
    }

    fn enumerate() -> Result<Vec<PortInfo>, TransportError> {
        let ports = SerialPort::available_ports()
            .map_err(|e| classify_io_error(&e, Op::Enumerate))?;
        Ok(ports
            .into_iter()
            .map(|p| PortInfo::named(p.to_string_lossy().into_owned()))
            .collect())
    }

    fn open(port_name: &str, config: OpenConfig) -> Result<Self, TransportError> {
        let mut inner = SerialPort::open(port_name, config.baud)
            .map_err(|e| classify_io_error(&e, Op::Open))?;
        inner
            .set_read_timeout(config.read_timeout)
            .map_err(|e| classify_io_error(&e, Op::Open))?;
        inner
            .set_write_timeout(config.write_timeout)
            .map_err(|e| classify_io_error(&e, Op::Open))?;
        Ok(Self { inner })
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        self.inner.read(buf).map_err(|e| classify_io_error(&e, Op::Read))
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, TransportError> {
        self.inner.write(buf).map_err(|e| classify_io_error(&e, Op::Write))
    }

    fn flush(&mut self) -> Result<(), TransportError> {
        self.inner.flush().map_err(|e| classify_io_error(&e, Op::Flush))
    }

    fn close(self) -> Result<(), TransportError> {
        drop(self.inner);
        Ok(())
    }
}

impl Serial2Backend {
    /// `serial2` takes `&self` for read and write, so a clone can be handed to another
    /// thread. This is the primitive a cancellation design would be built on.
    pub fn try_clone_handle(&self) -> Result<SerialPort, TransportError> {
        self.inner.try_clone().map_err(|e| classify_io_error(&e, Op::Open))
    }
}
