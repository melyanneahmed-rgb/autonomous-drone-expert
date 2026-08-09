import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_STORAGE_REVISION,
  classifyStorageException,
  decideCompareAndSwap,
  formatStorageRevision,
  isExpectedJournalObjectStoreSchema,
  isStorageRevision,
  isValidStorageKey,
  parseStorageRevision,
  validateStoredJournalRecord,
} from "../src/storage/journal-storage-contract.mjs";

const key = "case-0001";

function record(revision, bytes = [65, 68, 69, 74, 1, 0, 0, 0]) {
  return {
    key,
    schemaVersion: 1,
    revision,
    bytes: Uint8Array.from(bytes).buffer,
  };
}

test("storage keys mirror the Rust privacy-bounded contract", () => {
  for (const value of ["a", "case-0001", "case_9", "z".repeat(64)]) {
    assert.equal(isValidStorageKey(value), true, value);
  }
  for (const value of ["", "A", "case/1", "case.1", "é", "z".repeat(65), 1, null]) {
    assert.equal(isValidStorageKey(value), false, String(value));
  }
});

test("revision text is canonical and exact across the complete u64 domain", () => {
  assert.equal(parseStorageRevision("0"), 0n);
  assert.equal(parseStorageRevision("9007199254740993"), 9007199254740993n);
  assert.equal(parseStorageRevision("18446744073709551615"), MAX_STORAGE_REVISION);
  assert.equal(formatStorageRevision(MAX_STORAGE_REVISION), "18446744073709551615");
  assert.equal(isStorageRevision(MAX_STORAGE_REVISION), true);
  for (const value of ["", "00", "01", "+1", "-1", " 1", "1 ", "1.0", "1e3", "18446744073709551616", 1]) {
    assert.equal(parseStorageRevision(value), null, String(value));
  }
  assert.equal(formatStorageRevision(-1n), null);
  assert.equal(formatStorageRevision(MAX_STORAGE_REVISION + 1n), null);
});

test("stored records fail closed unless their complete schema is canonical", () => {
  const valid = validateStoredJournalRecord(record("7"), key);
  assert.deepEqual(valid.ok && valid.value.revision, 7n);
  assert.deepEqual(valid.ok && [...valid.value.bytes], [65, 68, 69, 74, 1, 0, 0, 0]);

  const malformed = [
    undefined,
    null,
    { ...record("7"), key: "another-case" },
    { ...record("7"), schemaVersion: 2 },
    { ...record("07") },
    { ...record("7"), bytes: new Uint8Array() },
    { ...record("7"), bytes: new ArrayBuffer(0) },
    { ...record("7"), extra: true },
  ];
  for (const value of malformed) {
    assert.deepEqual(validateStoredJournalRecord(value, key), { ok: false, failure: "Corrupt" });
  }
});

test("database object-store schema fails closed on key path, auto-increment, or index drift", () => {
  assert.equal(isExpectedJournalObjectStoreSchema("key", false, 0), true);
  assert.equal(isExpectedJournalObjectStoreSchema("revision", false, 0), false);
  assert.equal(isExpectedJournalObjectStoreSchema(["key"], false, 0), false);
  assert.equal(isExpectedJournalObjectStoreSchema("key", true, 0), false);
  assert.equal(isExpectedJournalObjectStoreSchema("key", false, 1), false);
});

test("CAS decision matrix rejects every absent, create, and stale mismatch", () => {
  assert.deepEqual(decideCompareAndSwap(undefined, null, key), { kind: "commit", revision: 1n });
  assert.deepEqual(decideCompareAndSwap(record("1"), null, key), { kind: "conflict" });
  assert.deepEqual(decideCompareAndSwap(undefined, 1n, key), { kind: "conflict" });
  assert.deepEqual(decideCompareAndSwap(record("2"), 1n, key), { kind: "conflict" });
  assert.deepEqual(decideCompareAndSwap(record("1"), 1n, key), { kind: "commit", revision: 2n });
  assert.deepEqual(decideCompareAndSwap(record("1"), 1, key), { kind: "failure", failure: "Unknown" });
});

test("revision exhaustion never wraps or invents successful progress", () => {
  assert.deepEqual(
    decideCompareAndSwap(record(MAX_STORAGE_REVISION.toString()), MAX_STORAGE_REVISION, key),
    { kind: "failure", failure: "Unknown" },
  );
});

test("browser errors map only to the stable Rust storage failure vocabulary", () => {
  assert.equal(classifyStorageException({ name: "QuotaExceededError" }), "QuotaExceeded");
  assert.equal(classifyStorageException({ name: "AbortError" }), "Cancelled");
  for (const name of ["VersionError", "InvalidStateError", "NotFoundError", "NotSupportedError", "SecurityError"]) {
    assert.equal(classifyStorageException({ name }), "Unavailable");
  }
  assert.equal(classifyStorageException({ name: "ConstraintError" }), "Unknown");
  assert.equal(classifyStorageException(new Error("opaque")), "Unknown");
});
