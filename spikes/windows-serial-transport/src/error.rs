//! The error model this project wants, and the mapping experiment.
//!
//! The question the spike must answer is not "does the library have errors" but
//! "how much of OUR error model can each library express, and how much must an
//! adapter of ours reconstruct?"

use std::fmt;
use std::io;

/// The transport error model proposed for production (documented in
/// `docs/TRANSPORT-CONTRACT.md`). Nothing here is production code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    PortNotFound,
    PortBusy,
    PermissionDenied,
    OpenFailed,
    ReadTimeout,
    WriteTimeout,
    ReadFailed,
    WriteFailed,
    DeviceDisconnected,
    OperationCancelled,
    UnsupportedConfiguration,
    UnknownTransportError,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::PortNotFound => "PORT_NOT_FOUND",
            Self::PortBusy => "PORT_BUSY",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::OpenFailed => "OPEN_FAILED",
            Self::ReadTimeout => "READ_TIMEOUT",
            Self::WriteTimeout => "WRITE_TIMEOUT",
            Self::ReadFailed => "READ_FAILED",
            Self::WriteFailed => "WRITE_FAILED",
            Self::DeviceDisconnected => "DEVICE_DISCONNECTED",
            Self::OperationCancelled => "OPERATION_CANCELLED",
            Self::UnsupportedConfiguration => "UNSUPPORTED_CONFIGURATION",
            Self::UnknownTransportError => "UNKNOWN_TRANSPORT_ERROR",
        };
        f.write_str(s)
    }
}

/// Which side of an operation the error came from. The same OS code means
/// `READ_TIMEOUT` or `WRITE_TIMEOUT` depending on what we were doing, so the caller
/// supplies the context rather than the library guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Enumerate,
    Open,
    Read,
    Write,
    Flush,
    Close,
}

/// Windows system error codes we care about.
///
/// Documented values from the Windows system error code list. Kept as named constants so
/// the mapping can be unit tested on any platform without a serial device present.
pub mod win32 {
    pub const ERROR_FILE_NOT_FOUND: i32 = 2;
    pub const ERROR_PATH_NOT_FOUND: i32 = 3;
    pub const ERROR_ACCESS_DENIED: i32 = 5;
    pub const ERROR_INVALID_HANDLE: i32 = 6;
    pub const ERROR_NOT_READY: i32 = 21;
    pub const ERROR_BAD_COMMAND: i32 = 22;
    pub const ERROR_SHARING_VIOLATION: i32 = 32;
    pub const ERROR_SEM_TIMEOUT: i32 = 121;
    pub const ERROR_OPERATION_ABORTED: i32 = 995;
    pub const ERROR_INVALID_PARAMETER: i32 = 87;
    pub const ERROR_DEVICE_NOT_CONNECTED: i32 = 1167;
    pub const ERROR_NO_SUCH_DEVICE: i32 = 433;
}

/// Map a raw OS error code to the model, given the operation being performed.
///
/// This is the part an adapter of ours must own regardless of which library wins:
/// neither candidate produces our vocabulary, and the distinction that matters most for
/// diagnosis — busy versus absent versus disconnected — is carried by the OS code.
pub fn classify_os_error(code: i32, op: Op) -> TransportError {
    use win32::*;
    match code {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_NO_SUCH_DEVICE => {
            TransportError::PortNotFound
        }
        ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION => TransportError::PortBusy,
        ERROR_INVALID_PARAMETER | ERROR_BAD_COMMAND => TransportError::UnsupportedConfiguration,
        ERROR_DEVICE_NOT_CONNECTED | ERROR_INVALID_HANDLE | ERROR_NOT_READY => {
            TransportError::DeviceDisconnected
        }
        ERROR_OPERATION_ABORTED => TransportError::OperationCancelled,
        ERROR_SEM_TIMEOUT => match op {
            Op::Write => TransportError::WriteTimeout,
            _ => TransportError::ReadTimeout,
        },
        _ => match op {
            Op::Open => TransportError::OpenFailed,
            Op::Read => TransportError::ReadFailed,
            Op::Write => TransportError::WriteFailed,
            _ => TransportError::UnknownTransportError,
        },
    }
}

/// Map a `std::io::Error` — what `serial2` returns everywhere.
pub fn classify_io_error(err: &io::Error, op: Op) -> TransportError {
    if let Some(code) = err.raw_os_error() {
        return classify_os_error(code, op);
    }
    match err.kind() {
        io::ErrorKind::NotFound => TransportError::PortNotFound,
        io::ErrorKind::PermissionDenied => TransportError::PermissionDenied,
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => match op {
            Op::Write => TransportError::WriteTimeout,
            _ => TransportError::ReadTimeout,
        },
        io::ErrorKind::Interrupted => TransportError::OperationCancelled,
        io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionAborted => {
            TransportError::DeviceDisconnected
        }
        io::ErrorKind::InvalidInput => TransportError::UnsupportedConfiguration,
        _ => match op {
            Op::Open => TransportError::OpenFailed,
            Op::Read => TransportError::ReadFailed,
            Op::Write => TransportError::WriteFailed,
            _ => TransportError::UnknownTransportError,
        },
    }
}

/// Map a `serialport::Error` — a library-specific type that must first be reduced to
/// something comparable. Note what is lost: `ErrorKind::NoDevice` covers both "the port
/// does not exist" and "another process holds it", which are different diagnoses for the
/// user. The raw OS code is recovered where the library preserved it.
pub fn classify_serialport_error(err: &serialport::Error, op: Op) -> TransportError {
    if let Some(code) = io::Error::from(err.clone()).raw_os_error() {
        return classify_os_error(code, op);
    }
    match err.kind() {
        serialport::ErrorKind::NoDevice => TransportError::PortNotFound,
        serialport::ErrorKind::InvalidInput => TransportError::UnsupportedConfiguration,
        serialport::ErrorKind::Io(kind) => {
            classify_io_error(&io::Error::from(kind), op)
        }
        serialport::ErrorKind::Unknown => match op {
            Op::Open => TransportError::OpenFailed,
            Op::Read => TransportError::ReadFailed,
            Op::Write => TransportError::WriteFailed,
            _ => TransportError::UnknownTransportError,
        },
    }
}
