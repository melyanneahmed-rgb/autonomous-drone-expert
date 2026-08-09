import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const runtime = fs.readFileSync(new URL("../../crates/runtime-ports/src/lib.rs", import.meta.url), "utf8");
const casebook = fs.readFileSync(new URL("../../crates/casebook/src/lib.rs", import.meta.url), "utf8");

test("browser adapter remains pinned to the authoritative Rust storage effects", () => {
  assert.match(runtime, /pub enum StorageEffect\s*\{/);
  assert.match(runtime, /Load\s*\{\s*key: StorageKey\s*\}/);
  assert.match(runtime, /CompareAndSwap\s*\{[\s\S]*expected_revision: Option<StorageRevision>[\s\S]*bytes: Vec<u8>/);
  assert.match(runtime, /pub struct StorageRevision\(u64\)/);
  assert.match(runtime, /1\.\.=64 lowercase ASCII letters, digits, `_` or `-`/);
});
test("stable Rust failure categories cannot drift silently", () => {
  const failure = runtime.match(/pub enum StorageFailure\s*\{([\s\S]*?)\n\}/)?.[1] ?? "";
  for (const name of ["Conflict", "QuotaExceeded", "Unavailable", "Corrupt", "Cancelled", "Unknown"]) {
    assert.match(failure, new RegExp(`\\b${name}\\b`), name);
  }
});

test("EffectJournalStore still owns load, CAS, and commit acceptance", () => {
  assert.match(casebook, /pub struct EffectJournalStore/);
  assert.match(casebook, /pub fn begin_load[\s\S]*StorageEffect::Load/);
  assert.match(casebook, /pub fn begin_append[\s\S]*StorageEffect::CompareAndSwap/);
  assert.match(casebook, /PREPARED is not DURABLY ACCEPTED/);
  assert.match(casebook, /pub fn accept_response/);
  assert.match(casebook, /loaded\.journal\.accept_prepared\(prepared\)/);
});
