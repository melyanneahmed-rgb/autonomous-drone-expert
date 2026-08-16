const equalBytes = (left, right) =>
  left.length === right.length && left.every((value, index) => value === right[index]);

const mark = (step) => {
  document.querySelector("#result").textContent = `INDEXEDDB_BROWSER_SMOKE_RUNNING:${step}`;
};

function assert(condition) {
  if (!condition) throw new Error("browser storage contract refusal");
}

function assertFailure(result, expected) {
  assert(result.ok === false && result.failure === expected);
}

async function runSmoke() {
  mark("import");
  const { IndexedDbJournalStore } = await import("./adapter.js");
  const mainKey = "browser-smoke-main";
  const raceKey = "browser-smoke-race";
  const absentKey = "browser-smoke-absent";
  const firstBytes = Uint8Array.from([65, 68, 69, 74, 1, 0, 0, 0]);
  const writerABytes = Uint8Array.from([...firstBytes, 1, 0, 0, 0, 2, 69, 96, 12, 7]);
  const writerBBytes = Uint8Array.from([...firstBytes, 1, 0, 0, 0, 3, 178, 49, 61, 1]);

  const readerA = new IndexedDbJournalStore();
  const readerB = new IndexedDbJournalStore();
  mark("missing-load");
  const missing = await readerA.load(mainKey);
  assert(missing.ok && missing.value === null);

  mark("create");
  const created = await readerA.compareAndSwap(mainKey, null, firstBytes);
  assert(created.ok && created.value === 1n);
  assertFailure(await readerB.compareAndSwap(mainKey, null, writerBBytes), "Conflict");
  assertFailure(await readerB.compareAndSwap(absentKey, 1n, writerBBytes), "Conflict");
  assertFailure(await readerB.compareAndSwap(absentKey, null, new Uint8Array()), "Corrupt");

  const loadedA = await readerA.load(mainKey);
  const loadedB = await readerB.load(mainKey);
  assert(loadedA.ok && loadedA.value?.revision === 1n);
  assert(loadedB.ok && loadedB.value?.revision === 1n);
  assert(equalBytes(loadedA.value.bytes, firstBytes));

  mark("update");
  const writerA = await readerA.compareAndSwap(mainKey, loadedA.value.revision, writerABytes);
  assert(writerA.ok && writerA.value === 2n);
  assertFailure(
    await readerB.compareAndSwap(mainKey, loadedB.value.revision, writerBBytes),
    "Conflict",
  );
  assertFailure(await readerB.compareAndSwap(mainKey, 1n, writerBBytes), "Conflict");
  const authoritative = await readerB.load(mainKey);
  assert(authoritative.ok && authoritative.value?.revision === 2n);
  assert(equalBytes(authoritative.value.bytes, writerABytes));

  mark("race");
  const raceA = new IndexedDbJournalStore();
  const raceB = new IndexedDbJournalStore();
  const race = await Promise.all([
    raceA.compareAndSwap(raceKey, null, writerABytes),
    raceB.compareAndSwap(raceKey, null, writerBBytes),
  ]);
  assert(race.filter((result) => result.ok).length === 1);
  assert(race.filter((result) => !result.ok && result.failure === "Conflict").length === 1);
  const raceWinner = race[0].ok ? writerABytes : writerBBytes;
  const raceLoaded = await raceA.load(raceKey);
  assert(raceLoaded.ok && raceLoaded.value?.revision === 1n);
  assert(equalBytes(raceLoaded.value.bytes, raceWinner));

  readerA.close();
  readerB.close();
  raceA.close();
  raceB.close();

  mark("reopen");
  const restarted = new IndexedDbJournalStore();
  const reopened = await restarted.load(mainKey);
  assert(reopened.ok && reopened.value?.revision === 2n);
  assert(equalBytes(reopened.value.bytes, writerABytes));
  restarted.close();
}

const output = document.querySelector("#result");
try {
  await runSmoke();
  document.body.dataset.result = "pass";
  output.textContent = "INDEXEDDB_BROWSER_SMOKE_PASS";
} catch (error) {
  document.body.dataset.result = "fail";
  const name = error instanceof Error ? error.name : "UnknownError";
  const message = error instanceof Error ? error.message : "unknown failure";
  output.textContent = `INDEXEDDB_BROWSER_SMOKE_FAIL:${name}:${message}`;
}
