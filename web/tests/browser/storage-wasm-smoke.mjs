import { IndexedDbJournalStore, JOURNAL_DATABASE_NAME, JOURNAL_DATABASE_VERSION, JOURNAL_OBJECT_STORE_NAME } from "/adapter.js";
import { WasmJournalHost } from "/wasm-journal-host.mjs";
import initWasm, { WasmJournalStore } from "/wasm/ade_web_storage_wasm_bridge.js";

const EMPTY_ADEJ = Uint8Array.from([65, 68, 69, 74, 1, 0, 0, 0]);

const mark = (step) => {
  document.querySelector("#result").textContent = `STORAGE_WASM_BROWSER_RUNNING:${step}`;
};

function assert(condition, message) {
  if (!condition) throw new Error(`storage WASM refusal: ${message}`);
}

function equalBytes(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function assertRefused(action, fragment) {
  try {
    action();
  } catch (error) {
    assert(String(error).includes(fragment), `wrong refusal: ${error}`);
    return;
  }
  throw new Error(`storage WASM refusal: expected ${fragment}`);
}

async function assertRejected(action, fragment) {
  try {
    await action();
  } catch (error) {
    assert(String(error).includes(fragment), `wrong rejection: ${error}`);
    return;
  }
  throw new Error(`storage WASM refusal: expected ${fragment}`);
}

function rawPutRecord(key, revision, bytes) {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(JOURNAL_DATABASE_NAME, JOURNAL_DATABASE_VERSION);
    open.addEventListener("error", () => reject(open.error), { once: true });
    open.addEventListener("success", () => {
      const database = open.result;
      const transaction = database.transaction(JOURNAL_OBJECT_STORE_NAME, "readwrite");
      transaction.addEventListener("abort", () => reject(transaction.error), { once: true });
      transaction.addEventListener("error", () => reject(transaction.error), { once: true });
      transaction.addEventListener("complete", () => {
        database.close();
        resolve();
      }, { once: true });
      transaction.objectStore(JOURNAL_OBJECT_STORE_NAME).put({
        key,
        schemaVersion: 1,
        revision,
        bytes: Uint8Array.from(bytes).buffer,
      });
    }, { once: true });
  });
}

async function scenarioEmptyLoad(adapter) {
  mark("A-empty-load");
  const bridge = new WasmJournalStore("wasm-empty-load", 4096);
  try {
    const host = new WasmJournalHost(bridge, adapter);
    assert(await host.load() === "loaded", "empty load outcome");
    assert(bridge.eventCount === 0, "empty load event count");
    assert(bridge.revision === undefined, "empty load revision");
    assert(bridge.hasPending === false, "empty load pending state");
  } finally {
    bridge.free();
  }
}

async function scenarioTornRepair(adapter) {
  mark("B-torn-tail-repair");
  const key = "wasm-torn-repair";
  const seed = new WasmJournalStore(key, 4096);
  let validBytes;
  try {
    const seedHost = new WasmJournalHost(seed, adapter);
    await seedHost.load();
    const append = seed.beginAppendMarker("identity-read");
    try {
      validBytes = Uint8Array.from(append.bytes);
    } finally {
      append.free();
    }
  } finally {
    seed.free();
  }

  const tornBytes = Uint8Array.from([...validBytes, 4, 0, 0, 0, 9]);
  const seeded = await adapter.compareAndSwap(key, null, tornBytes);
  assert(seeded.ok && seeded.value === 1n, "torn fixture commit");

  const repair = new WasmJournalStore(key, 4096);
  try {
    const repairHost = new WasmJournalHost(repair, adapter);
    assert(await repairHost.load() === "repair-committed", "Rust repair outcome");
    assert(repair.eventCount === 1, "repair event count");
    assert(repair.revision === "2", "repair revision");
  } finally {
    repair.free();
  }

  const reopened = new WasmJournalStore(key, 4096);
  try {
    const reopenHost = new WasmJournalHost(reopened, adapter);
    assert(await reopenHost.load() === "loaded", "clean reopen outcome");
    assert(reopened.eventCount === 1, "clean reopen event count");
    assert(reopened.revision === "2", "clean reopen revision");
    const stored = await adapter.load(key);
    assert(stored.ok && stored.value?.revision === 2n, "stored repaired revision");
    assert(equalBytes(stored.value.bytes, validBytes), "Rust-selected repaired bytes");
  } finally {
    reopened.free();
  }
}

async function scenarioStaleCas(adapter) {
  mark("C-stale-cas");
  const key = "wasm-stale-cas";
  const writer = new WasmJournalStore(key, 4096);
  try {
    const writerHost = new WasmJournalHost(writer, adapter);
    await writerHost.load();
    await writerHost.appendMarker("identity-read");
    assert(writer.revision === "1", "writer seed revision");
  } finally {
    writer.free();
  }

  const stale = new WasmJournalStore(key, 4096);
  try {
    const staleHost = new WasmJournalHost(stale, adapter);
    await staleHost.load();
    const pending = stale.beginAppendMarker("snapshot-read");
    const competingBytes = Uint8Array.from(pending.bytes);
    const competing = await adapter.compareAndSwap(key, 1n, competingBytes);
    assert(competing.ok && competing.value === 2n, "competing CAS");
    await assertRejected(() => staleHost.execute(pending), "Conflict");
    assert(stale.eventCount === 1, "stale event was not accepted");
    assert(stale.revision === "1", "stale Rust revision was not invented");
    assert(stale.hasPending === false, "matching conflict consumed operation");
  } finally {
    stale.free();
  }
}

function scenarioResponseRefusals() {
  mark("D-response-refusals");
  const bridge = new WasmJournalStore("wasm-response-refusal", 4096);
  try {
    const load = bridge.beginLoad();
    try {
      assertRefused(() => bridge.acceptLoadMissing("2"), "RequestIdMismatch");
      assert(bridge.hasPending, "wrong id preserved pending request");
      assertRefused(
        () => bridge.acceptCommitSuccess(load.requestId, "1"),
        "ResponseKindMismatch",
      );
      assert(bridge.hasPending, "wrong kind preserved pending request");
      assert(bridge.acceptLoadMissing(load.requestId) === "loaded", "correct response");
      assertRefused(() => bridge.acceptLoadMissing(load.requestId), "NoStorageRequestPending");
    } finally {
      load.free();
    }
  } finally {
    bridge.free();
  }
}

async function scenarioExactU64(adapter) {
  mark("E-exact-u64");
  const key = "wasm-u64-exact";
  await rawPutRecord(key, "9007199254740993", EMPTY_ADEJ);
  const bridge = new WasmJournalStore(key, 4096);
  try {
    const host = new WasmJournalHost(bridge, adapter);
    await host.load();
    assert(bridge.revision === "9007199254740993", "high revision load text");
    const append = bridge.beginAppendMarker("identity-read");
    assert(append.expectedRevision === "9007199254740993", "high expected revision text");
    assert(await host.execute(append) === "append-committed", "high revision append");
    assert(bridge.revision === "9007199254740994", "high revision commit text");
    const stored = await adapter.load(key);
    assert(stored.ok && stored.value?.revision === 9007199254740994n, "high IndexedDB bigint");
  } finally {
    bridge.free();
  }
}

async function run() {
  mark("initialize-real-wasm");
  await initWasm({
    module_or_path: new URL(
      "/wasm/ade_web_storage_wasm_bridge_bg.wasm",
      globalThis.location.href,
    ),
  });
  const adapter = new IndexedDbJournalStore();
  try {
    await scenarioEmptyLoad(adapter);
    await scenarioTornRepair(adapter);
    await scenarioStaleCas(adapter);
    scenarioResponseRefusals();
    await scenarioExactU64(adapter);
  } finally {
    adapter.close();
  }
}

const output = document.querySelector("#result");
try {
  await run();
  document.body.dataset.result = "pass";
  output.textContent = "STORAGE_WASM_BROWSER_PASS:A+B+C+D+E";
} catch (error) {
  document.body.dataset.result = "fail";
  const name = error instanceof Error ? error.name : "UnknownError";
  const message = error instanceof Error ? error.message : String(error);
  output.textContent = `STORAGE_WASM_BROWSER_FAIL:${name}:${message}`;
}
