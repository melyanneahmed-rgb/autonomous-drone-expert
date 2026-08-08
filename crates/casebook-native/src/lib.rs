#![forbid(unsafe_code)]

//! Native filesystem durability for the backend-neutral `ade-casebook` ADEJ codec.
//!
//! This crate is deliberately native-only. It owns file creation, append, flush, sync, reopen
//! validation, and the narrowly defined repair of an incomplete final record.

use ade_casebook::{
    JOURNAL_HEADER_LEN, Journal, JournalBackend, JournalError, PreparedJournalAppend,
    empty_journal_bytes,
};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

#[derive(Debug)]
struct NativeBackend<F> {
    file: F,
}

trait DurableIo: fmt::Debug {
    fn durable_len(&self) -> std::io::Result<u64>;
    fn seek_end(&mut self) -> std::io::Result<()>;
    fn write_record(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    fn flush_record(&mut self) -> std::io::Result<()>;
    fn sync_record(&mut self) -> std::io::Result<()>;
}

impl DurableIo for File {
    fn durable_len(&self) -> std::io::Result<u64> {
        Ok(self.metadata()?.len())
    }

    fn seek_end(&mut self) -> std::io::Result<()> {
        self.seek(SeekFrom::End(0)).map(|_| ())
    }

    fn write_record(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.write_all(bytes)
    }

    fn flush_record(&mut self) -> std::io::Result<()> {
        self.flush()
    }

    fn sync_record(&mut self) -> std::io::Result<()> {
        self.sync_data()
    }
}

impl<F: DurableIo> JournalBackend for NativeBackend<F> {
    fn append_durable(&mut self, prepared: &PreparedJournalAppend) -> Result<(), JournalError> {
        let actual = usize::try_from(self.file.durable_len()?).map_err(|_| {
            JournalError::BackendPositionMismatch {
                expected: prepared.expected_len(),
                actual: usize::MAX,
            }
        })?;
        if actual != prepared.expected_len() {
            return Err(JournalError::BackendPositionMismatch {
                expected: prepared.expected_len(),
                actual,
            });
        }

        self.file.seek_end()?;
        self.file.write_record(prepared.record_bytes())?;
        self.file.flush_record()?;
        self.file.sync_record()?;
        Ok(())
    }
}

/// Create a new durable journal without overwriting an existing path.
///
/// # Errors
/// Returns [`JournalError::AlreadyExists`] for an existing path, or a stable format/I/O error.
pub fn create_new(path: impl AsRef<Path>, max_bytes: usize) -> Result<Journal, JournalError> {
    if max_bytes < JOURNAL_HEADER_LEN {
        return Err(JournalError::LimitTooSmall);
    }
    let path = path.as_ref();
    let mut file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(JournalError::AlreadyExists);
        }
        Err(error) => return Err(error.into()),
    };
    file.write_all(&empty_journal_bytes())?;
    file.flush()?;
    file.sync_all()?;

    Ok(Journal::with_limit(max_bytes)?.with_backend(NativeBackend { file }))
}

/// Open and validate a durable journal, or create it when the path does not exist.
///
/// Only an incomplete final record is repaired. Complete corruption is refused without mutation.
///
/// # Errors
/// Returns a stable format, bound, or I/O error.
pub fn open(path: impl AsRef<Path>, max_bytes: usize) -> Result<Journal, JournalError> {
    if max_bytes < JOURNAL_HEADER_LEN {
        return Err(JournalError::LimitTooSmall);
    }
    let path = path.as_ref().to_path_buf();
    let mut file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return create_new(path, max_bytes);
        }
        Err(error) => return Err(error.into()),
    };

    let file_len = usize::try_from(file.metadata()?.len())
        .map_err(|_| JournalError::Full { limit: max_bytes })?;
    if file_len > max_bytes {
        return Err(JournalError::Full { limit: max_bytes });
    }
    let mut bytes = Vec::with_capacity(file_len);
    file.read_to_end(&mut bytes)?;
    let decoded = Journal::decode(&bytes, max_bytes)?;
    if let Some(repair_to) = decoded.repair_to() {
        file.set_len(repair_to as u64)?;
        file.seek(SeekFrom::Start(repair_to as u64))?;
        file.flush()?;
        file.sync_data()?;
    } else {
        file.seek(SeekFrom::End(0))?;
    }
    Ok(decoded.into_journal().with_backend(NativeBackend { file }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_casebook::JournalEvent;
    use std::cell::RefCell;
    use std::fs;
    use std::io::ErrorKind;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    #[derive(Debug)]
    struct TestPath(PathBuf);

    impl TestPath {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                "ade-casebook-native-{label}-{}-{nonce}-{sequence}.adej",
                std::process::id()
            );
            Self(std::env::temp_dir().join(name))
        }
    }

    impl Drop for TestPath {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn durable_identity_journal(path: &Path) -> Vec<u8> {
        let mut journal = create_new(path, 1024).unwrap();
        journal.try_append(JournalEvent::IdentityRead).unwrap();
        drop(journal);
        fs::read(path).unwrap()
    }

    #[test]
    fn create_new_syncs_header_and_never_overwrites() {
        let path = TestPath::new("create");
        let journal = create_new(&path.0, 1024).unwrap();
        assert_eq!(journal.encoded_len(), JOURNAL_HEADER_LEN);
        assert_eq!(fs::read(&path.0).unwrap(), empty_journal_bytes());
        assert_eq!(
            create_new(&path.0, 1024).unwrap_err(),
            JournalError::AlreadyExists
        );
    }

    #[test]
    fn append_is_durable_before_logical_accept_and_reopens_validated() {
        let path = TestPath::new("roundtrip");
        let bytes = durable_identity_journal(&path.0);
        let reopened = open(&path.0, 1024).unwrap();
        assert_eq!(reopened.events(), &[JournalEvent::IdentityRead]);
        assert_eq!(reopened.encoded_len(), bytes.len());
    }

    #[test]
    fn only_an_incomplete_final_record_is_repaired() {
        let path = TestPath::new("tail");
        let mut bytes = durable_identity_journal(&path.0);
        let proven_len = bytes.len();
        bytes.extend([4, 0, 0, 0, 9]);
        fs::write(&path.0, &bytes).unwrap();

        let reopened = open(&path.0, 1024).unwrap();
        assert_eq!(reopened.events(), &[JournalEvent::IdentityRead]);
        assert_eq!(reopened.encoded_len(), proven_len);
        assert_eq!(fs::metadata(&path.0).unwrap().len(), proven_len as u64);
    }

    #[test]
    fn checksum_and_complete_middle_corruption_are_refused_without_mutation() {
        let path = TestPath::new("corrupt");
        let mut journal = create_new(&path.0, 1024).unwrap();
        journal.try_append(JournalEvent::IdentityRead).unwrap();
        journal.try_append(JournalEvent::SnapshotRead).unwrap();
        drop(journal);
        let mut bytes = fs::read(&path.0).unwrap();
        bytes[JOURNAL_HEADER_LEN + 4] ^= 0xff;
        fs::write(&path.0, &bytes).unwrap();
        assert_eq!(
            open(&path.0, 1024).unwrap_err(),
            JournalError::ChecksumMismatch {
                offset: JOURNAL_HEADER_LEN
            }
        );
        assert_eq!(fs::read(&path.0).unwrap(), bytes);
    }

    #[test]
    fn invalid_magic_and_version_are_refused_without_mutation() {
        for (label, bytes, expected) in [
            (
                "magic",
                vec![b'B', b'A', b'D', b'!', 1, 0, 0, 0],
                JournalError::InvalidMagic,
            ),
            (
                "version",
                vec![b'A', b'D', b'E', b'J', 2, 0, 0, 0],
                JournalError::UnsupportedVersion(2),
            ),
        ] {
            let path = TestPath::new(label);
            fs::write(&path.0, &bytes).unwrap();
            assert_eq!(open(&path.0, 1024).unwrap_err(), expected);
            assert_eq!(fs::read(&path.0).unwrap(), bytes);
        }
    }

    #[test]
    fn backend_position_mismatch_does_not_advance_and_poisons_handle() {
        let path = TestPath::new("position");
        let mut journal = create_new(&path.0, 1024).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&path.0)
            .unwrap()
            .write_all(&[0])
            .unwrap();
        assert_eq!(
            journal.try_append(JournalEvent::IdentityRead),
            Err(JournalError::BackendPositionMismatch {
                expected: JOURNAL_HEADER_LEN,
                actual: JOURNAL_HEADER_LEN + 1,
            })
        );
        assert!(journal.events().is_empty());
        assert_eq!(journal.encoded_len(), JOURNAL_HEADER_LEN);
        assert_eq!(
            journal.try_append(JournalEvent::SnapshotRead),
            Err(JournalError::Poisoned)
        );
    }

    #[test]
    fn uncertain_native_write_failure_requires_reopen_before_reuse() {
        let path = TestPath::new("failure");
        fs::write(&path.0, empty_journal_bytes()).unwrap();
        let read_only = File::open(&path.0).unwrap();
        let mut journal = Journal::new().with_backend(NativeBackend { file: read_only });
        assert_eq!(
            journal.try_append(JournalEvent::IdentityRead),
            Err(JournalError::Io(ErrorKind::PermissionDenied))
        );
        assert!(journal.events().is_empty());
        assert_eq!(
            journal.try_append(JournalEvent::SnapshotRead),
            Err(JournalError::Poisoned)
        );
        drop(journal);

        let reopened = open(&path.0, 1024).unwrap();
        assert!(reopened.events().is_empty());
    }

    #[test]
    fn bounds_are_checked_before_creating_or_mutating_files() {
        let absent = TestPath::new("small-absent");
        assert_eq!(
            create_new(&absent.0, JOURNAL_HEADER_LEN - 1).unwrap_err(),
            JournalError::LimitTooSmall
        );
        assert!(!absent.0.exists());

        let existing = TestPath::new("small-existing");
        fs::write(&existing.0, b"sentinel").unwrap();
        assert_eq!(
            open(&existing.0, JOURNAL_HEADER_LEN - 1).unwrap_err(),
            JournalError::LimitTooSmall
        );
        assert_eq!(fs::read(&existing.0).unwrap(), b"sentinel");
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum FailurePhase {
        Write,
        Flush,
        Sync,
    }

    #[derive(Debug)]
    struct FaultState {
        bytes: Vec<u8>,
        calls: Vec<&'static str>,
    }

    #[derive(Debug)]
    struct FaultIo {
        phase: FailurePhase,
        state: Rc<RefCell<FaultState>>,
    }

    impl DurableIo for FaultIo {
        fn durable_len(&self) -> std::io::Result<u64> {
            let mut state = self.state.borrow_mut();
            state.calls.push("len");
            Ok(state.bytes.len() as u64)
        }

        fn seek_end(&mut self) -> std::io::Result<()> {
            self.state.borrow_mut().calls.push("seek");
            Ok(())
        }

        fn write_record(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            let mut state = self.state.borrow_mut();
            state.calls.push("write");
            if self.phase == FailurePhase::Write {
                state.bytes.extend(&bytes[..bytes.len() / 2]);
                return Err(std::io::Error::other("injected write failure"));
            }
            state.bytes.extend(bytes);
            Ok(())
        }

        fn flush_record(&mut self) -> std::io::Result<()> {
            self.state.borrow_mut().calls.push("flush");
            if self.phase == FailurePhase::Flush {
                return Err(std::io::Error::other("injected flush failure"));
            }
            Ok(())
        }

        fn sync_record(&mut self) -> std::io::Result<()> {
            self.state.borrow_mut().calls.push("sync");
            if self.phase == FailurePhase::Sync {
                return Err(std::io::Error::other("injected sync failure"));
            }
            Ok(())
        }
    }

    #[test]
    fn write_flush_and_sync_failures_never_advance_logical_state() {
        for (phase, expected_calls) in [
            (FailurePhase::Write, vec!["len", "seek", "write"]),
            (FailurePhase::Flush, vec!["len", "seek", "write", "flush"]),
            (
                FailurePhase::Sync,
                vec!["len", "seek", "write", "flush", "sync"],
            ),
        ] {
            let state = Rc::new(RefCell::new(FaultState {
                bytes: empty_journal_bytes().to_vec(),
                calls: Vec::new(),
            }));
            let mut journal = Journal::new().with_backend(NativeBackend {
                file: FaultIo {
                    phase,
                    state: Rc::clone(&state),
                },
            });

            assert_eq!(
                journal.try_append(JournalEvent::IdentityRead),
                Err(JournalError::Io(ErrorKind::Other))
            );
            assert!(journal.events().is_empty());
            assert_eq!(journal.encoded_len(), JOURNAL_HEADER_LEN);
            assert_eq!(state.borrow().calls, expected_calls);
            assert_eq!(
                journal.try_append(JournalEvent::SnapshotRead),
                Err(JournalError::Poisoned)
            );
        }
    }
}
