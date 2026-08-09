export const STORAGE_RECORD_SCHEMA_VERSION = 1;
export const MAX_STORAGE_REVISION = 18446744073709551615n;

const STORAGE_KEY_PATTERN = /^[a-z0-9_-]{1,64}$/;
const CANONICAL_REVISION_PATTERN = /^(?:0|[1-9][0-9]*)$/;
const RECORD_FIELDS = ["bytes", "key", "revision", "schemaVersion"];

export function isValidStorageKey(value) {
  return typeof value === "string" && STORAGE_KEY_PATTERN.test(value);
}

export function isStorageRevision(value) {
  return typeof value === "bigint" && value >= 0n && value <= MAX_STORAGE_REVISION;
}

export function parseStorageRevision(value) {
  if (typeof value !== "string" || !CANONICAL_REVISION_PATTERN.test(value)) {
    return null;
  }
  const parsed = BigInt(value);
  return parsed <= MAX_STORAGE_REVISION ? parsed : null;
}

export function formatStorageRevision(value) {
  return isStorageRevision(value) ? value.toString(10) : null;
}

export function isExpectedJournalObjectStoreSchema(keyPath, autoIncrement, indexCount) {
  return keyPath === "key" && autoIncrement === false && indexCount === 0;
}

export function classifyStorageException(error) {
  const name =
    typeof error === "object" && error !== null && typeof error.name === "string"
      ? error.name
      : "";
  if (name === "QuotaExceededError") return "QuotaExceeded";
  if (name === "AbortError") return "Cancelled";
  if (
    name === "VersionError" ||
    name === "InvalidStateError" ||
    name === "NotFoundError" ||
    name === "NotSupportedError" ||
    name === "SecurityError"
  ) {
    return "Unavailable";
  }
  return "Unknown";
}

export function validateStoredJournalRecord(record, expectedKey) {
  if (!isValidStorageKey(expectedKey) || typeof record !== "object" || record === null) {
    return { ok: false, failure: "Corrupt" };
  }
  if (Object.getPrototypeOf(record) !== Object.prototype) {
    return { ok: false, failure: "Corrupt" };
  }
  const fields = Object.keys(record).sort();
  if (fields.length !== RECORD_FIELDS.length || fields.some((field, index) => field !== RECORD_FIELDS[index])) {
    return { ok: false, failure: "Corrupt" };
  }
  if (record.key !== expectedKey || record.schemaVersion !== STORAGE_RECORD_SCHEMA_VERSION) {
    return { ok: false, failure: "Corrupt" };
  }
  const revision = parseStorageRevision(record.revision);
  if (revision === null || !(record.bytes instanceof ArrayBuffer) || record.bytes.byteLength === 0) {
    return { ok: false, failure: "Corrupt" };
  }
  return {
    ok: true,
    value: {
      revision,
      bytes: new Uint8Array(record.bytes.slice(0)),
    },
  };
}

export function decideCompareAndSwap(record, expectedRevision, expectedKey) {
  if (expectedRevision !== null && !isStorageRevision(expectedRevision)) {
    return { kind: "failure", failure: "Unknown" };
  }
  if (record === undefined) {
    return expectedRevision === null
      ? { kind: "commit", revision: 1n }
      : { kind: "conflict" };
  }
  const validated = validateStoredJournalRecord(record, expectedKey);
  if (!validated.ok) return { kind: "failure", failure: validated.failure };
  if (expectedRevision === null || validated.value.revision !== expectedRevision) {
    return { kind: "conflict" };
  }
  if (validated.value.revision === MAX_STORAGE_REVISION) {
    return { kind: "failure", failure: "Unknown" };
  }
  return { kind: "commit", revision: validated.value.revision + 1n };
}
