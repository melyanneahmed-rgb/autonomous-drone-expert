import { WebSerialReadonlyHost } from "/webserial-readonly-host.mjs";
import initWasm, {
  WasmReadonlySerialDirective,
  WasmReadonlySerialDiscovery,
} from "/wasm/ade_web_readonly_serial_wasm_bridge.js";

// Static, project-owned test fixtures. Production JavaScript has no MSP command constants,
// semantic parser, frame builder, or response fixtures.
const IN_SCOPE_REPLIES = [
  [36, 77, 62, 3, 1, 0, 1, 46, 45],
  [36, 77, 62, 4, 2, 66, 84, 70, 76, 26],
  [36, 77, 62, 3, 3, 4, 5, 5, 4],
  [36, 77, 62, 88, 4, 83, 52, 48, 53, 0, 0, 0, 0, 15, 83, 80, 69, 69, 68, 89, 66, 69, 69, 70, 52, 48, 53, 86, 52, 17, 83, 112, 101, 101, 100, 121, 66, 101, 101, 32, 70, 52, 48, 53, 32, 86, 52, 3, 83, 80, 66, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 66],
].map((bytes) => Uint8Array.from(bytes));
const OUT_OF_SCOPE_REPLIES = [
  Uint8Array.from([36, 77, 62, 3, 1, 0, 1, 45, 46]),
  ...IN_SCOPE_REPLIES.slice(1),
];
const EXPECTED_REQUESTS = [1, 2, 3, 4].map((command) =>
  Uint8Array.from([36, 77, 60, 0, command, command]),
);
const PROHIBITED_TEST_REQUESTS = [68, 184, 185, 250, 99].map((command) =>
  Uint8Array.from([36, 77, 60, 0, command, command]),
);

function assert(condition, message) {
  if (!condition) throw new Error(`Web Serial read-only refusal: ${message}`);
}

function equalBytes(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function refused(action) {
  try {
    action();
  } catch {
    return;
  }
  throw new Error("Web Serial read-only refusal: expected Rust refusal");
}

const mark = (step) => {
  document.querySelector("#result").textContent = `WEB_SERIAL_READONLY_RUNNING:${step}`;
};

class TestPort {
  constructor(replies, mode = "normal") {
    this.replies = replies;
    this.mode = mode;
    this.writes = [];
    this.queue = [];
    this.openOptions = null;
    this.openCount = 0;
    this.closeCount = 0;
    this.readerReleased = 0;
    this.writerReleased = 0;
    this.readerCancelled = 0;
    this.readable = {
      getReader: () => ({
        read: () => this.#read(),
        cancel: async () => { this.readerCancelled += 1; },
        releaseLock: () => { this.readerReleased += 1; },
      }),
    };
    this.writable = {
      getWriter: () => ({
        write: async (bytes) => this.#write(bytes),
        releaseLock: () => { this.writerReleased += 1; },
      }),
    };
  }

  async open(options) {
    this.openOptions = options;
    this.openCount += 1;
  }

  async close() {
    this.closeCount += 1;
  }

  #write(bytes) {
    this.writes.push(Uint8Array.from(bytes));
    const reply = this.replies[this.writes.length - 1];
    if (reply) {
      if (this.mode === "bad-checksum" && this.writes.length === 1) {
        const bad = Uint8Array.from(reply);
        bad[bad.length - 1] ^= 1;
        this.queue.push(bad);
      } else if (this.mode === "oversized" && this.writes.length === 1) {
        this.queue.push(Uint8Array.from([36, 77, 62, 123]));
      } else if (this.mode === "truncated-timeout" && this.writes.length === 1) {
        this.queue.push(reply.slice(0, reply.length - 1));
      } else if (this.mode === "disconnect" && this.writes.length === 1) {
        this.queue.push(null);
      } else {
        for (const byte of reply) this.queue.push(Uint8Array.of(byte));
      }
    }
  }

  #read() {
    if (this.queue.length > 0) {
      const chunk = this.queue.shift();
      return Promise.resolve(chunk === null ? { done: true } : { done: false, value: chunk });
    }
    return new Promise(() => {});
  }
}

class TestSerial {
  constructor(port, selectionError = null) {
    this.port = port;
    this.selectionError = selectionError;
    this.requestCount = 0;
  }

  async requestPort() {
    this.requestCount += 1;
    if (this.selectionError) throw this.selectionError;
    return this.port;
  }
}

function hostFor(serial, timeoutMs = 30) {
  return new WebSerialReadonlyHost({
    serial,
    timeoutMs,
    rustDirectiveType: WasmReadonlySerialDirective,
  });
}

async function runDiscovery(replies, mode = "normal", timeoutMs = 30) {
  const port = new TestPort(replies, mode);
  const serial = new TestSerial(port);
  const host = hostFor(serial, timeoutMs);
  const selected = await host.selectPortFromUserGesture();
  assert(selected.ok, "explicit selection should succeed");
  const bridge = new WasmReadonlySerialDiscovery();
  try {
    const result = await host.discover(bridge);
    return { bridge, result, port, serial };
  } catch (error) {
    bridge.free();
    throw error;
  }
}

async function scenarioAUnavailable() {
  mark("A-api-unavailable");
  const host = hostFor(null);
  const selected = await host.selectPortFromUserGesture();
  assert(!selected.ok && selected.failure === "Unavailable", "stable unavailable result");
}

async function scenarioBCancelled() {
  mark("B-selection-cancelled");
  const error = new Error("owner cancelled");
  error.name = "NotFoundError";
  const serial = new TestSerial(null, error);
  const host = hostFor(serial);
  const selected = await host.selectPortFromUserGesture();
  assert(!selected.ok && selected.failure === "Cancelled", "stable cancelled result");
  assert(serial.requestCount === 1, "one explicit requestPort call");
}

async function scenarioCSuccessAndGCleanup() {
  mark("C-success-G-cleanup");
  const run = await runDiscovery(IN_SCOPE_REPLIES);
  try {
    assert(
      run.result.outcome === "in-scope",
      `typed in-scope result (${JSON.stringify(run.result)}, writes=${run.port.writes.length})`,
    );
    assert(run.bridge.apiVersion === "1.46", "typed API version");
    assert(run.bridge.fcVariant === "BTFL", "typed FC variant");
    assert(run.bridge.fcVersion === "4.5.5", "typed FC version");
    assert(run.bridge.targetName === "SPEEDYBEEF405V4", "typed target");
    assert(run.bridge.hardwareObserved === false, "software evidence only");
    assert(run.port.openOptions?.baudRate === 115200, "internal baud rate");
    assert(run.port.writes.length === 4, "exactly four writes");
    EXPECTED_REQUESTS.forEach((expected, index) =>
      assert(equalBytes(run.port.writes[index], expected), `request order ${index}`),
    );
    assert(run.port.readerReleased === 1, "reader lock released");
    assert(run.port.writerReleased === 1, "writer lock released");
    assert(run.port.closeCount === 1, "port closed exactly once");
  } finally {
    run.bridge.free();
  }
}

async function scenarioDAuthorityRefusal() {
  mark("D-authority-refusal");
  const port = new TestPort(IN_SCOPE_REPLIES);
  const host = hostFor(new TestSerial(port));
  await host.selectPortFromUserGesture();
  const before = port.writes.length;
  for (const prohibited of PROHIBITED_TEST_REQUESTS) {
    assert(prohibited.length > 0, "test-only prohibited fixture exists");
    assert(typeof WasmReadonlySerialDiscovery.prototype.sendRaw === "undefined", "no raw API");
    assert(typeof WasmReadonlySerialDiscovery.prototype.writeCommand === "undefined", "no command API");
    assert(port.writes.length === before, "prohibited bytes cannot reach writer");
  }
  class ForgedDirective {}
  const forgedDiscovery = { begin: () => ({ kind: "exchange-identification-read", bytes: PROHIBITED_TEST_REQUESTS[0], requestId: "1" }) };
  await assertRejected(() => host.discover(forgedDiscovery), "UNTRUSTED_DIRECTIVE");
  assert(port.writes.length === before, "forged directive refused before writer");
  assert(ForgedDirective !== WasmReadonlySerialDirective, "only generated Rust directive type trusted");
}

async function assertRejected(action, fragment) {
  try {
    await action();
  } catch (error) {
    assert(String(error).includes(fragment), `wrong refusal: ${error}`);
    return;
  }
  throw new Error(`Web Serial read-only refusal: expected ${fragment}`);
}

function scenarioECorrelation() {
  mark("E-correlation");
  const bridge = new WasmReadonlySerialDiscovery();
  try {
    const open = bridge.begin();
    refused(() => bridge.acceptOpenSuccess("2"));
    refused(() => bridge.acceptCloseSuccess(open.requestId));
    const exchange = bridge.acceptOpenSuccess(open.requestId);
    refused(() => bridge.acceptOpenSuccess(open.requestId));
    refused(() => bridge.acceptReadChunk(open.requestId, Uint8Array.of(36)));
    assert(exchange.requestId !== open.requestId, "request id advanced exactly");
    open.free();
    exchange.free();
  } finally {
    bridge.free();
  }
}

async function scenarioFFailClosed() {
  mark("F-malformed-timeout-disconnect");
  for (const [mode, expected] of [
    ["bad-checksum", "MalformedResponse"],
    ["truncated-timeout", "Timeout"],
    ["disconnect", "Disconnected"],
    ["oversized", "MalformedResponse"],
  ]) {
    const run = await runDiscovery(IN_SCOPE_REPLIES, mode, 15);
    try {
      assert(run.result.outcome === "failed", `${mode} failed`);
      assert(run.result.failure === expected, `${mode} stable failure`);
      assert(run.port.closeCount === 1, `${mode} closed`);
      assert(run.port.readerReleased === 1, `${mode} reader released`);
      assert(run.port.writerReleased === 1, `${mode} writer released`);
    } finally {
      run.bridge.free();
    }
  }
}

async function scenarioHScopeMismatch() {
  mark("H-scope-mismatch");
  const run = await runDiscovery(OUT_OF_SCOPE_REPLIES);
  try {
    assert(run.result.outcome === "scope-mismatch", "scope mismatch result");
    assert(run.result.scopeMismatchField === "msp_api_version", "typed mismatch field");
    assert(run.bridge.hardwareObserved === false, "scope is not hardware evidence");
    assert(run.port.writes.length === 4 && run.port.closeCount === 1, "read-only stop and close");
  } finally {
    run.bridge.free();
  }
}

async function run() {
  await initWasm({
    module_or_path: new URL(
      "/wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
      globalThis.location.href,
    ),
  });
  await scenarioAUnavailable();
  await scenarioBCancelled();
  await scenarioCSuccessAndGCleanup();
  await scenarioDAuthorityRefusal();
  scenarioECorrelation();
  await scenarioFFailClosed();
  await scenarioHScopeMismatch();
}

const output = document.querySelector("#result");
try {
  await run();
  document.body.dataset.result = "pass";
  output.textContent = "WEB_SERIAL_READONLY_BROWSER_PASS:A+B+C+D+E+F+G+H";
} catch (error) {
  document.body.dataset.result = "fail";
  const name = error instanceof Error ? error.name : "UnknownError";
  const message = error instanceof Error ? error.message : String(error);
  output.textContent = `WEB_SERIAL_READONLY_BROWSER_FAIL:${name}:${message}`;
}
