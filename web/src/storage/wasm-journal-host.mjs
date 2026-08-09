const MAX_U64 = 18446744073709551615n;
const CANONICAL_DECIMAL = /^(?:0|[1-9][0-9]*)$/;
const STORAGE_FAILURES = new Set([
  "Conflict",
  "QuotaExceeded",
  "Unavailable",
  "Corrupt",
  "Cancelled",
  "Unknown",
]);

export class StorageWasmHostProtocolError extends Error {
  constructor(message) {
    super(`STORAGE_WASM_HOST_REFUSAL:${message}`);
    this.name = "StorageWasmHostProtocolError";
  }
}

function refuse(message) {
  throw new StorageWasmHostProtocolError(message);
}

export function parseCanonicalU64(value, label = "U64") {
  if (typeof value !== "string" || !CANONICAL_DECIMAL.test(value)) {
    refuse(`INVALID_${label}_DECIMAL`);
  }
  const parsed = BigInt(value);
  if (parsed > MAX_U64) refuse(`INVALID_${label}_DECIMAL`);
  return parsed;
}

export function formatCanonicalU64(value, label = "U64") {
  if (typeof value !== "bigint" || value < 0n || value > MAX_U64) {
    refuse(`INVALID_${label}`);
  }
  return value.toString(10);
}

function requireRequestId(value) {
  const parsed = parseCanonicalU64(value, "REQUEST_ID");
  if (parsed === 0n) refuse("INVALID_REQUEST_ID_DECIMAL");
  return value;
}

function requireStorageFailure(value) {
  if (!STORAGE_FAILURES.has(value)) refuse("INVALID_STORAGE_FAILURE");
  return value;
}

function requireOutcome(actual, expected) {
  if (actual !== expected) refuse(`UNEXPECTED_RUST_OUTCOME:${actual}`);
  return actual;
}

/**
 * Drives Rust-owned storage effects against a byte/CAS host adapter.
 *
 * This class never constructs a journal record, invents a request id, advances a revision,
 * or accepts an event. It only translates exact decimal revisions to/from `bigint` at the
 * IndexedDB adapter boundary and returns every result to the Rust coordinator.
 */
export class WasmJournalHost {
  #bridge;
  #store;

  constructor(bridge, store) {
    if (typeof bridge !== "object" || bridge === null) refuse("INVALID_RUST_BRIDGE");
    if (typeof store !== "object" || store === null) refuse("INVALID_STORAGE_ADAPTER");
    this.#bridge = bridge;
    this.#store = store;
  }

  async execute(directive) {
    if (typeof directive !== "object" || directive === null) {
      refuse("INVALID_STORAGE_DIRECTIVE");
    }
    const requestId = requireRequestId(directive.requestId);
    if (typeof directive.key !== "string") refuse("INVALID_STORAGE_KEY");

    if (directive.kind === "load") {
      return this.#executeLoad(directive, requestId);
    }
    if (directive.kind === "compare-and-swap") {
      return this.#executeCompareAndSwap(directive, requestId);
    }
    refuse("NON_STORAGE_DIRECTIVE");
  }

  async load() {
    const outcome = await this.execute(this.#bridge.beginLoad());
    if (outcome === "loaded") return outcome;
    if (outcome !== "repair-required") {
      refuse(`UNEXPECTED_RUST_OUTCOME:${outcome}`);
    }
    const repaired = await this.execute(this.#bridge.takeRepairEffect());
    return requireOutcome(repaired, "repair-committed");
  }

  async appendMarker(marker) {
    if (typeof marker !== "string") refuse("INVALID_APPEND_MARKER");
    const outcome = await this.execute(this.#bridge.beginAppendMarker(marker));
    return requireOutcome(outcome, "append-committed");
  }

  async #executeLoad(directive, requestId) {
    if (
      directive.expectedRevision !== undefined ||
      !(directive.bytes instanceof Uint8Array) ||
      directive.bytes.byteLength !== 0
    ) {
      refuse("MALFORMED_LOAD_DIRECTIVE");
    }
    const result = await this.#store.load(directive.key);
    if (!result.ok) {
      return this.#bridge.acceptLoadFailure(
        requestId,
        requireStorageFailure(result.failure),
      );
    }
    if (result.value === null) {
      return this.#bridge.acceptLoadMissing(requestId);
    }
    if (!(result.value.bytes instanceof Uint8Array)) refuse("INVALID_LOADED_BYTES");
    return this.#bridge.acceptLoadFound(
      requestId,
      formatCanonicalU64(result.value.revision, "REVISION"),
      Uint8Array.from(result.value.bytes),
    );
  }

  async #executeCompareAndSwap(directive, requestId) {
    if (!(directive.bytes instanceof Uint8Array) || directive.bytes.byteLength === 0) {
      refuse("INVALID_COMMIT_BYTES");
    }
    const expectedRevision =
      directive.expectedRevision === undefined
        ? null
        : parseCanonicalU64(directive.expectedRevision, "REVISION");
    const result = await this.#store.compareAndSwap(
      directive.key,
      expectedRevision,
      Uint8Array.from(directive.bytes),
    );
    if (!result.ok) {
      return this.#bridge.acceptCommitFailure(
        requestId,
        requireStorageFailure(result.failure),
      );
    }
    return this.#bridge.acceptCommitSuccess(
      requestId,
      formatCanonicalU64(result.value, "REVISION"),
    );
  }
}
