import { IndexedDbJournalStore, JOURNAL_DATABASE_NAME, JOURNAL_DATABASE_VERSION, JOURNAL_OBJECT_STORE_NAME } from "./adapter.js";
import { WasmJournalHost } from "./wasm-journal-host.mjs";
import initWasm, { WasmJournalStore } from "./wasm/ade_web_storage_wasm_bridge.js";

// Fixed test-only ADEJ fixture: header plus one IdentityRead record. Production JavaScript
// never constructs journal records or chooses JournalEvent values.
const VALID_ONE_EVENT_ADEJ = Uint8Array.from([
  65, 68, 69, 74, 1, 0, 0, 0,
  1, 0, 0, 0, 2, 69, 96, 12, 7,
]);
const TORN_ONE_EVENT_ADEJ = Uint8Array.from([
  ...VALID_ONE_EVENT_ADEJ,
  4, 0, 0, 0, 9,
]);

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
  const seeded = await adapter.compareAndSwap(key, null, TORN_ONE_EVENT_ADEJ);
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
    assert(
      equalBytes(stored.value.bytes, VALID_ONE_EVENT_ADEJ),
      "Rust-selected repaired bytes",
    );
  } finally {
    reopened.free();
  }
}

async function scenarioStaleCas(adapter) {
  mark("C-stale-cas");
  const key = "wasm-stale-cas";
  await rawPutRecord(key, "1", TORN_ONE_EVENT_ADEJ);

  const stale = new WasmJournalStore(key, 4096);
  try {
    const staleHost = new WasmJournalHost(stale, adapter);
    assert(
      await staleHost.execute(stale.beginLoad()) === "repair-required",
      "Rust emitted repair requirement",
    );
    const pending = stale.takeRepairEffect();
    assert(pending.expectedRevision === "1", "repair expected seeded revision");
    const competing = await adapter.compareAndSwap(key, 1n, VALID_ONE_EVENT_ADEJ);
    assert(competing.ok && competing.value === 2n, "competing CAS");
    await assertRejected(() => staleHost.execute(pending), "Conflict");
    assert(stale.eventCount === 0, "stale repair was not accepted");
    assert(stale.revision === undefined, "stale Rust revision was not invented");
    assert(stale.hasPending === false, "matching conflict consumed operation");
    const stored = await adapter.load(key);
    assert(stored.ok && stored.value?.revision === 2n, "competing revision retained");
    assert(equalBytes(stored.value.bytes, VALID_ONE_EVENT_ADEJ), "no last-write-wins");
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
  await rawPutRecord(key, "9007199254740993", TORN_ONE_EVENT_ADEJ);
  const bridge = new WasmJournalStore(key, 4096);
  try {
    const host = new WasmJournalHost(bridge, adapter);
    assert(
      await host.execute(bridge.beginLoad()) === "repair-required",
      "high revision repair requirement",
    );
    const repair = bridge.takeRepairEffect();
    assert(repair.expectedRevision === "9007199254740993", "high expected revision text");
    assert(await host.execute(repair) === "repair-committed", "high revision repair");
    assert(bridge.revision === "9007199254740994", "high revision commit text");
    const stored = await adapter.load(key);
    assert(stored.ok && stored.value?.revision === 9007199254740994n, "high IndexedDB bigint");
    assert(equalBytes(stored.value.bytes, VALID_ONE_EVENT_ADEJ), "high revision repaired bytes");
  } finally {
    bridge.free();
  }

  const reopened = new WasmJournalStore(key, 4096);
  try {
    const host = new WasmJournalHost(reopened, adapter);
    assert(await host.load() === "loaded", "high revision exact reopen");
    assert(reopened.revision === "9007199254740994", "high reopen revision text");
    assert(reopened.eventCount === 1, "high reopen event count");
  } finally {
    reopened.free();
  }
}

async function run() {
  mark("initialize-real-wasm");
  await initWasm({
    module_or_path: new URL(
      "./wasm/ade_web_storage_wasm_bridge_bg.wasm",
      globalThis.location.href,
    ),
  });
  assert(
    typeof WasmJournalStore.prototype.beginAppendMarker === "undefined",
    "production WASM exposes no JournalEvent append API",
  );
  assert(
    typeof WasmJournalHost.prototype.appendMarker === "undefined",
    "production host exposes no JournalEvent append API",
  );
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
