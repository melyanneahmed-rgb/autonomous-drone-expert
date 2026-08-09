#![forbid(unsafe_code)]

//! Storage-only WebAssembly facade for [`ade_casebook::EffectJournalStore`].
//!
//! JavaScript remains a byte/CAS host. It receives only typed storage directives and returns
//! typed storage results. Request ids and revisions cross the ABI as canonical decimal text,
//! never as JavaScript `Number`; the Rust coordinator remains the authority that accepts or
//! refuses stale, duplicate, or wrong-kind responses.

use std::fmt;

use ade_casebook::{EffectJournalError, EffectJournalOutcome, EffectJournalStore};
use ade_runtime_ports::{
    IoEffect, IoResponse, RequestId, StorageEffect, StorageFailure, StorageKey, StorageResult,
    StorageRevision, StoredValue,
};
use wasm_bindgen::prelude::*;

const OUTCOME_LOADED: &str = "loaded";
const OUTCOME_REPAIR_REQUIRED: &str = "repair-required";
const OUTCOME_APPEND_COMMITTED: &str = "append-committed";
const OUTCOME_REPAIR_COMMITTED: &str = "repair-committed";

#[derive(Debug)]
enum BridgeError {
    Core(EffectJournalError),
    InvalidDecimal(&'static str),
    InvalidStorageFailure,
    NonStorageEffect,
    NoRepairEffect,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => write!(formatter, "RUST_STORAGE_REFUSAL:{error}"),
            Self::InvalidDecimal(label) => {
                write!(formatter, "RUST_STORAGE_REFUSAL:INVALID_{label}_DECIMAL")
            }
            Self::InvalidStorageFailure => {
                formatter.write_str("RUST_STORAGE_REFUSAL:INVALID_STORAGE_FAILURE")
            }
            Self::NonStorageEffect => {
                formatter.write_str("RUST_STORAGE_REFUSAL:NON_STORAGE_EFFECT")
            }
            Self::NoRepairEffect => formatter.write_str("RUST_STORAGE_REFUSAL:NO_REPAIR_EFFECT"),
        }
    }
}

impl From<EffectJournalError> for BridgeError {
    fn from(error: EffectJournalError) -> Self {
        Self::Core(error)
    }
}

fn js_error(error: BridgeError) -> JsError {
    JsError::new(&error.to_string())
}

fn parse_decimal(value: &str, label: &'static str) -> Result<u64, BridgeError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(BridgeError::InvalidDecimal(label));
    }
    value
        .parse::<u64>()
        .map_err(|_| BridgeError::InvalidDecimal(label))
}

fn parse_failure(value: &str) -> Result<StorageFailure, BridgeError> {
    match value {
        "Conflict" => Ok(StorageFailure::Conflict),
        "QuotaExceeded" => Ok(StorageFailure::QuotaExceeded),
        "Unavailable" => Ok(StorageFailure::Unavailable),
        "Corrupt" => Ok(StorageFailure::Corrupt),
        "Cancelled" => Ok(StorageFailure::Cancelled),
        "Unknown" => Ok(StorageFailure::Unknown),
        _ => Err(BridgeError::InvalidStorageFailure),
    }
}

/// One host storage operation emitted by the Rust state machine.
#[wasm_bindgen]
pub struct WasmStorageDirective {
    request_id: String,
    kind: &'static str,
    key: String,
    expected_revision: Option<String>,
    bytes: Vec<u8>,
}

#[wasm_bindgen]
impl WasmStorageDirective {
    #[wasm_bindgen(getter, js_name = requestId)]
    pub fn request_id(&self) -> String {
        self.request_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.to_owned()
    }

    #[wasm_bindgen(getter)]
    pub fn key(&self) -> String {
        self.key.clone()
    }

    #[wasm_bindgen(getter, js_name = expectedRevision)]
    pub fn expected_revision(&self) -> Option<String> {
        self.expected_revision.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

fn directive_from(effect: IoEffect) -> Result<WasmStorageDirective, BridgeError> {
    match effect {
        IoEffect::Storage {
            request_id,
            effect: StorageEffect::Load { key },
        } => Ok(WasmStorageDirective {
            request_id: request_id.get().to_string(),
            kind: "load",
            key: key.as_str().to_owned(),
            expected_revision: None,
            bytes: Vec::new(),
        }),
        IoEffect::Storage {
            request_id,
            effect:
                StorageEffect::CompareAndSwap {
                    key,
                    expected_revision,
                    bytes,
                },
        } => Ok(WasmStorageDirective {
            request_id: request_id.get().to_string(),
            kind: "compare-and-swap",
            key: key.as_str().to_owned(),
            expected_revision: expected_revision.map(|revision| revision.get().to_string()),
            bytes,
        }),
        IoEffect::Transport { .. } => Err(BridgeError::NonStorageEffect),
    }
}

/// Rust-owned journal storage state exposed through a storage-only WebAssembly ABI.
#[wasm_bindgen]
pub struct WasmJournalStore {
    store: EffectJournalStore,
    repair_effect: Option<WasmStorageDirective>,
}

impl WasmJournalStore {
    fn create(key: &str, max_bytes: u32) -> Result<Self, BridgeError> {
        let key = StorageKey::new(key).map_err(EffectJournalError::from)?;
        let store = EffectJournalStore::new(key, max_bytes as usize)?;
        Ok(Self {
            store,
            repair_effect: None,
        })
    }

    fn do_begin_load(&mut self) -> Result<WasmStorageDirective, BridgeError> {
        directive_from(self.store.begin_load()?)
    }

    fn accept(&mut self, response: IoResponse) -> Result<&'static str, BridgeError> {
        match self.store.accept_response(response)? {
            EffectJournalOutcome::Loaded => Ok(OUTCOME_LOADED),
            EffectJournalOutcome::RepairRequired(effect) => {
                self.repair_effect = Some(directive_from(effect)?);
                Ok(OUTCOME_REPAIR_REQUIRED)
            }
            EffectJournalOutcome::AppendCommitted => Ok(OUTCOME_APPEND_COMMITTED),
            EffectJournalOutcome::RepairCommitted => Ok(OUTCOME_REPAIR_COMMITTED),
        }
    }

    fn request_id(value: &str) -> Result<RequestId, BridgeError> {
        Ok(RequestId::new(parse_decimal(value, "REQUEST_ID")?))
    }

    fn revision(value: &str) -> Result<StorageRevision, BridgeError> {
        Ok(StorageRevision::new(parse_decimal(value, "REVISION")?))
    }

    fn do_accept_load_missing(&mut self, request_id: &str) -> Result<&'static str, BridgeError> {
        self.accept(IoResponse::Storage {
            request_id: Self::request_id(request_id)?,
            result: StorageResult::Load(Ok(None)),
        })
    }

    fn do_accept_load_found(
        &mut self,
        request_id: &str,
        revision: &str,
        bytes: Vec<u8>,
    ) -> Result<&'static str, BridgeError> {
        self.accept(IoResponse::Storage {
            request_id: Self::request_id(request_id)?,
            result: StorageResult::Load(Ok(Some(StoredValue {
                revision: Self::revision(revision)?,
                bytes,
            }))),
        })
    }

    fn do_accept_load_failure(
        &mut self,
        request_id: &str,
        failure: &str,
    ) -> Result<&'static str, BridgeError> {
        self.accept(IoResponse::Storage {
            request_id: Self::request_id(request_id)?,
            result: StorageResult::Load(Err(parse_failure(failure)?)),
        })
    }

    fn do_accept_commit_success(
        &mut self,
        request_id: &str,
        revision: &str,
    ) -> Result<&'static str, BridgeError> {
        self.accept(IoResponse::Storage {
            request_id: Self::request_id(request_id)?,
            result: StorageResult::Commit(Ok(Self::revision(revision)?)),
        })
    }

    fn do_accept_commit_failure(
        &mut self,
        request_id: &str,
        failure: &str,
    ) -> Result<&'static str, BridgeError> {
        self.accept(IoResponse::Storage {
            request_id: Self::request_id(request_id)?,
            result: StorageResult::Commit(Err(parse_failure(failure)?)),
        })
    }

    fn take_repair(&mut self) -> Result<WasmStorageDirective, BridgeError> {
        self.repair_effect.take().ok_or(BridgeError::NoRepairEffect)
    }

    #[cfg(test)]
    fn begin_append_for_test(
        &mut self,
        event: ade_casebook::JournalEvent,
    ) -> Result<WasmStorageDirective, BridgeError> {
        directive_from(self.store.begin_append(event)?)
    }
}

#[wasm_bindgen]
impl WasmJournalStore {
    #[wasm_bindgen(constructor)]
    pub fn new(key: &str, max_bytes: u32) -> Result<WasmJournalStore, JsError> {
        Self::create(key, max_bytes).map_err(js_error)
    }

    #[wasm_bindgen(js_name = beginLoad)]
    pub fn begin_load(&mut self) -> Result<WasmStorageDirective, JsError> {
        self.do_begin_load().map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptLoadMissing)]
    pub fn accept_load_missing(&mut self, request_id: &str) -> Result<String, JsError> {
        self.do_accept_load_missing(request_id)
            .map(str::to_owned)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptLoadFound)]
    pub fn accept_load_found(
        &mut self,
        request_id: &str,
        revision: &str,
        bytes: Vec<u8>,
    ) -> Result<String, JsError> {
        self.do_accept_load_found(request_id, revision, bytes)
            .map(str::to_owned)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptLoadFailure)]
    pub fn accept_load_failure(
        &mut self,
        request_id: &str,
        failure: &str,
    ) -> Result<String, JsError> {
        self.do_accept_load_failure(request_id, failure)
            .map(str::to_owned)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptCommitSuccess)]
    pub fn accept_commit_success(
        &mut self,
        request_id: &str,
        revision: &str,
    ) -> Result<String, JsError> {
        self.do_accept_commit_success(request_id, revision)
            .map(str::to_owned)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = acceptCommitFailure)]
    pub fn accept_commit_failure(
        &mut self,
        request_id: &str,
        failure: &str,
    ) -> Result<String, JsError> {
        self.do_accept_commit_failure(request_id, failure)
            .map(str::to_owned)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = takeRepairEffect)]
    pub fn take_repair_effect(&mut self) -> Result<WasmStorageDirective, JsError> {
        self.take_repair().map_err(js_error)
    }

    #[wasm_bindgen(getter, js_name = hasPending)]
    pub fn has_pending(&self) -> bool {
        self.store.has_pending()
    }

    #[wasm_bindgen(getter, js_name = eventCount)]
    pub fn event_count(&self) -> u32 {
        self.store
            .journal()
            .map_or(0, |journal| journal.events().len() as u32)
    }

    #[wasm_bindgen(getter, js_name = revision)]
    pub fn revision_text(&self) -> Option<String> {
        self.store
            .revision()
            .map(|revision| revision.get().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_casebook::JournalEvent;

    const EMPTY_ADEJ: &[u8] = b"ADEJ\x01\x00\x00\x00";

    fn unloaded(key: &str) -> WasmJournalStore {
        WasmJournalStore::create(key, 4096).unwrap()
    }

    fn empty_loaded(key: &str) -> WasmJournalStore {
        let mut bridge = unloaded(key);
        let load = bridge.do_begin_load().unwrap();
        assert_eq!(
            bridge.do_accept_load_missing(&load.request_id).unwrap(),
            OUTCOME_LOADED
        );
        bridge
    }

    #[test]
    fn directives_use_canonical_decimal_text_and_storage_only_kinds() {
        let mut bridge = empty_loaded("wasm-directive");
        let directive = bridge
            .begin_append_for_test(JournalEvent::IdentityRead)
            .unwrap();
        assert_eq!(directive.request_id, "2");
        assert_eq!(directive.kind, "compare-and-swap");
        assert_eq!(directive.key, "wasm-directive");
        assert_eq!(directive.expected_revision, None);
        assert!(directive.bytes.starts_with(EMPTY_ADEJ));
    }

    #[test]
    fn wrong_id_wrong_kind_and_duplicate_responses_are_refused() {
        let mut bridge = unloaded("wasm-refusal");
        let load = bridge.do_begin_load().unwrap();

        assert!(bridge.do_accept_load_missing("2").is_err());
        assert!(bridge.store.has_pending());
        assert!(
            bridge
                .do_accept_commit_success(&load.request_id, "1")
                .is_err()
        );
        assert!(bridge.store.has_pending());
        assert_eq!(
            bridge.do_accept_load_missing(&load.request_id).unwrap(),
            OUTCOME_LOADED
        );
        assert!(bridge.do_accept_load_missing(&load.request_id).is_err());
    }

    #[test]
    fn rust_owns_torn_tail_repair_before_journal_becomes_visible() {
        let mut seed = empty_loaded("wasm-repair");
        let append = seed
            .begin_append_for_test(JournalEvent::IdentityRead)
            .unwrap();
        let mut torn = append.bytes.clone();
        torn.extend([4, 0, 0, 0, 9]);

        let mut reopened = unloaded("wasm-repair");
        let load = reopened.do_begin_load().unwrap();
        assert_eq!(
            reopened
                .do_accept_load_found(&load.request_id, "4", torn)
                .unwrap(),
            OUTCOME_REPAIR_REQUIRED
        );
        assert_eq!(reopened.event_count(), 0);
        assert_eq!(reopened.revision_text(), None);

        let repair = reopened.take_repair().unwrap();
        assert_eq!(repair.kind, "compare-and-swap");
        assert_eq!(repair.expected_revision.as_deref(), Some("4"));
        assert_eq!(repair.bytes, append.bytes);
        assert_eq!(
            reopened
                .do_accept_commit_success(&repair.request_id, "5")
                .unwrap(),
            OUTCOME_REPAIR_COMMITTED
        );
        assert_eq!(reopened.event_count(), 1);
        assert_eq!(reopened.revision_text().as_deref(), Some("5"));
    }

    #[test]
    fn stale_cas_failure_never_accepts_the_prepared_event() {
        let mut bridge = unloaded("wasm-conflict");
        let load = bridge.do_begin_load().unwrap();
        bridge
            .do_accept_load_found(&load.request_id, "7", EMPTY_ADEJ.to_vec())
            .unwrap();
        let append = bridge
            .begin_append_for_test(JournalEvent::SnapshotRead)
            .unwrap();
        assert!(
            bridge
                .do_accept_commit_failure(&append.request_id, "Conflict")
                .is_err()
        );
        assert_eq!(bridge.event_count(), 0);
        assert_eq!(bridge.revision_text().as_deref(), Some("7"));
        assert!(!bridge.store.has_pending());
    }

    #[test]
    fn revisions_above_javascript_safe_integer_remain_exact() {
        let mut bridge = unloaded("wasm-u64");
        let load = bridge.do_begin_load().unwrap();
        bridge
            .do_accept_load_found(&load.request_id, "9007199254740993", EMPTY_ADEJ.to_vec())
            .unwrap();
        assert_eq!(bridge.revision_text().as_deref(), Some("9007199254740993"));
        let append = bridge
            .begin_append_for_test(JournalEvent::IdentityRead)
            .unwrap();
        assert_eq!(
            append.expected_revision.as_deref(),
            Some("9007199254740993")
        );
        assert!(parse_decimal("09007199254740993", "REVISION").is_err());
        assert!(parse_decimal("18446744073709551616", "REVISION").is_err());
    }
}
