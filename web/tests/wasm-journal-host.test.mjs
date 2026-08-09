import assert from "node:assert/strict";
import test from "node:test";

import {
  StorageWasmHostProtocolError,
  WasmJournalHost,
  formatCanonicalU64,
  parseCanonicalU64,
} from "../src/storage/wasm-journal-host.mjs";

const EMPTY_ADEJ = Uint8Array.from([65, 68, 69, 74, 1, 0, 0, 0]);

function loadDirective(requestId = "1") {
  return {
    requestId,
    kind: "load",
    key: "host-test",
    expectedRevision: undefined,
    bytes: new Uint8Array(),
  };
}

function casDirective(expectedRevision = undefined, requestId = "2") {
  return {
    requestId,
    kind: "compare-and-swap",
    key: "host-test",
    expectedRevision,
    bytes: EMPTY_ADEJ,
  };
}

test("u64 conversions remain canonical and exact above the JavaScript safe integer", () => {
  const value = parseCanonicalU64("9007199254740993", "REVISION");
  assert.equal(value, 9007199254740993n);
  assert.equal(formatCanonicalU64(value, "REVISION"), "9007199254740993");
  for (const invalid of ["", "01", "-1", "1.0", 9007199254740993]) {
    assert.throws(
      () => parseCanonicalU64(invalid, "REVISION"),
      StorageWasmHostProtocolError,
    );
  }
  assert.throws(
    () => parseCanonicalU64("18446744073709551616", "REVISION"),
    StorageWasmHostProtocolError,
  );
});

test("missing and found loads are returned to Rust without inventing state", async () => {
  const calls = [];
  const bridge = {
    beginLoad: () => loadDirective(),
    acceptLoadMissing: (requestId) => (calls.push(["missing", requestId]), "loaded"),
    acceptLoadFound: (requestId, revision, bytes) => (
      calls.push(["found", requestId, revision, [...bytes]]), "loaded"
    ),
  };
  const missingHost = new WasmJournalHost(bridge, {
    load: async () => ({ ok: true, value: null }),
  });
  assert.equal(await missingHost.load(), "loaded");

  const foundHost = new WasmJournalHost(bridge, {
    load: async () => ({
      ok: true,
      value: { revision: 9007199254740993n, bytes: EMPTY_ADEJ },
    }),
  });
  assert.equal(await foundHost.load(), "loaded");
  assert.deepEqual(calls, [
    ["missing", "1"],
    ["found", "1", "9007199254740993", [...EMPTY_ADEJ]],
  ]);
});

test("repair is a second Rust-emitted CAS and not a JavaScript journal rewrite", async () => {
  const calls = [];
  const bridge = {
    beginLoad: () => loadDirective(),
    acceptLoadFound: () => "repair-required",
    takeRepairEffect: () => casDirective("7", "2"),
    acceptCommitSuccess: (requestId, revision) => (
      calls.push([requestId, revision]), "repair-committed"
    ),
  };
  const store = {
    load: async () => ({ ok: true, value: { revision: 7n, bytes: EMPTY_ADEJ } }),
    compareAndSwap: async (_key, expectedRevision, bytes) => {
      assert.equal(expectedRevision, 7n);
      assert.deepEqual(bytes, EMPTY_ADEJ);
      return { ok: true, value: 8n };
    },
  };
  assert.equal(await new WasmJournalHost(bridge, store).load(), "repair-committed");
  assert.deepEqual(calls, [["2", "8"]]);
});

test("CAS revisions cross the host only as bigint and canonical decimal text", async () => {
  const calls = [];
  const bridge = {
    beginAppendMarker: () => casDirective("9007199254740993"),
    acceptCommitSuccess: (requestId, revision) => (
      calls.push([requestId, revision]), "append-committed"
    ),
  };
  const store = {
    compareAndSwap: async (_key, expectedRevision) => {
      assert.equal(expectedRevision, 9007199254740993n);
      return { ok: true, value: 9007199254740994n };
    },
  };
  const host = new WasmJournalHost(bridge, store);
  assert.equal(await host.appendMarker("identity-read"), "append-committed");
  assert.deepEqual(calls, [["2", "9007199254740994"]]);
});

test("unknown directives and malformed load/CAS shapes fail closed", async () => {
  const host = new WasmJournalHost({}, {});
  await assert.rejects(
    host.execute({ ...loadDirective(), kind: "transport" }),
    /NON_STORAGE_DIRECTIVE/,
  );
  await assert.rejects(
    host.execute({ ...loadDirective(), expectedRevision: "0" }),
    /MALFORMED_LOAD_DIRECTIVE/,
  );
  await assert.rejects(
    host.execute({ ...casDirective(), bytes: new Uint8Array() }),
    /INVALID_COMMIT_BYTES/,
  );
});
