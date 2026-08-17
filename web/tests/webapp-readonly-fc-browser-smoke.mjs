import fs from "node:fs";
import http from "node:http";
import { mkdtemp, rm } from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const dist = path.join(webRoot, "dist");
const servedRequests = [];

if (!fs.existsSync(path.join(dist, "index.html"))) {
  console.error("PRODUCTION_SMART_CONFIGURATOR_DIST_MISSING");
  process.exit(2);
}

const MIME = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".svg", "image/svg+xml"],
  [".wasm", "application/wasm"],
  [".webmanifest", "application/manifest+json"],
]);

function findBrowser() {
  const candidates = [
    process.env.CHROME_PATH,
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  ].filter(Boolean);
  for (const candidate of candidates) {
    if (path.isAbsolute(candidate) && fs.existsSync(candidate)) return candidate;
    const probe = spawnSync(candidate, ["--version"], { encoding: "utf8", timeout: 5000 });
    if (!probe.error && probe.status === 0) return candidate;
  }
  return null;
}

function serveProduction() {
  const root = path.resolve(dist);
  return http.createServer((request, response) => {
    const requestUrl = new URL(request.url, "http://127.0.0.1");
    servedRequests.push(requestUrl.pathname);
    const relative = requestUrl.pathname === "/" ? "index.html" : requestUrl.pathname.slice(1);
    const file = path.resolve(root, relative);
    if (!file.startsWith(`${root}${path.sep}`) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      response.writeHead(requestUrl.pathname === "/favicon.ico" ? 204 : 404);
      response.end();
      return;
    }
    response.writeHead(200, {
      "Content-Type": MIME.get(path.extname(file)) ?? "application/octet-stream",
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
    });
    response.end(fs.readFileSync(file));
  });
}

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

function installFakeSerial(scenario) {
  const inScopeReplies = [
    [36, 77, 62, 3, 1, 0, 1, 46, 45],
    [36, 77, 62, 4, 2, 66, 84, 70, 76, 26],
    [36, 77, 62, 3, 3, 4, 5, 5, 4],
    [36, 77, 62, 88, 4, 83, 52, 48, 53, 0, 0, 0, 0, 15, 83, 80, 69, 69, 68, 89, 66, 69, 69, 70, 52, 48, 53, 86, 52, 17, 83, 112, 101, 101, 100, 121, 66, 101, 101, 32, 70, 52, 48, 53, 32, 86, 52, 3, 83, 80, 66, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 66],
  ].map((bytes) => Uint8Array.from(bytes));
  const boardPayloadWithTrailingByte = [...inScopeReplies[3].slice(5, -1), 0];
  let boardChecksum = boardPayloadWithTrailingByte.length ^ 4;
  for (const byte of boardPayloadWithTrailingByte) boardChecksum ^= byte;
  const boardTrailingPayloadReply = Uint8Array.from([
    36, 77, 62, boardPayloadWithTrailingByte.length, 4,
    ...boardPayloadWithTrailingByte, boardChecksum,
  ]);
  const replies = scenario === "scope-mismatch"
    ? [Uint8Array.from([36, 77, 62, 3, 1, 0, 1, 45, 46]), ...inScopeReplies.slice(1)]
    : scenario === "protocol-identity-failure"
      ? [...inScopeReplies.slice(0, 3), boardTrailingPayloadReply]
      : inScopeReplies;

  class FakePort {
    constructor() {
      this.writes = [];
      this.queue = [];
      this.openCount = 0;
      this.closeCount = 0;
      this.readerReleased = 0;
      this.writerReleased = 0;
      this.readerCancelled = 0;
      this.openOptions = null;
      this.serialNumber = "SERIAL-SECRET-123";
      this.usbVendorId = "VID_1234";
      this.usbProductId = "PID_ABCD";
      this.path = "COM99:/private/path/raw-device-name";
      this.readable = {
        getReader: () => ({
          read: () => this.queue.length > 0
            ? Promise.resolve({ done: false, value: this.queue.shift() })
            : new Promise(() => {}),
          cancel: async () => { this.readerCancelled += 1; },
          releaseLock: () => { this.readerReleased += 1; },
        }),
      };
      this.writable = {
        getWriter: () => ({
          write: async (bytes) => {
            this.writes.push(Array.from(bytes));
            const reply = replies[this.writes.length - 1];
            if (reply) this.queue.push(reply);
          },
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
  }

  const port = new FakePort();
  const serial = {
    requestCount: 0,
    userActivationAtRequest: false,
    async requestPort() {
      this.requestCount += 1;
      this.userActivationAtRequest = navigator.userActivation?.isActive === true;
      if (scenario === "cancelled") {
        const error = new Error(
          "COM99 SERIAL-SECRET-123 VID_1234 PID_ABCD /private/path raw-device-name",
        );
        error.name = "NotFoundError";
        throw error;
      }
      return port;
    },
  };
  Object.defineProperty(navigator, "serial", {
    configurable: true,
    value: scenario === "unavailable" ? undefined : serial,
  });
  const clipboardWrites = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: {
      async writeText(value) {
        clipboardWrites.push(value);
      },
    },
  });
  globalThis.__ADE_SERIAL_PROBE__ = { scenario, serial, port, clipboardWrites };
}

function fakeSerialSource(scenario) {
  return `(${installFakeSerial.toString()})(${JSON.stringify(scenario)});`;
}

async function devtoolsSocket(devToolsUrl) {
  const endpoint = new URL(devToolsUrl);
  let target;
  for (let attempt = 0; attempt < 100 && !target; attempt += 1) {
    try {
      const targets = await fetch(`http://${endpoint.host}/json/list`).then((response) => response.json());
      target = targets.find((candidate) => candidate.type === "page");
    } catch {}
    if (!target) await delay(50);
  }
  if (!target?.webSocketDebuggerUrl) throw new Error("PRODUCTION_BROWSER_TARGET_UNAVAILABLE");
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let id = 0;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    if (message.error) waiter.reject(new Error(message.error.message));
    else waiter.resolve(message.result);
  });
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    id += 1;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
  const evaluate = async (expression) => {
    const response = await send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (response.exceptionDetails) throw new Error(response.exceptionDetails.text);
    return response.result.value;
  };
  return { socket, send, evaluate };
}

async function waitFor(evaluate, expression, expected, label) {
  let last;
  for (let attempt = 0; attempt < 300; attempt += 1) {
    last = await evaluate(expression);
    if (last === expected) return;
    await delay(100);
  }
  throw new Error(`${label}: expected ${expected}, got ${last}`);
}

async function waitForTerminalPhase(evaluate, label) {
  const terminal = new Set(["read-complete", "scope-mismatch", "cancelled", "unavailable", "failed"]);
  let last;
  for (let attempt = 0; attempt < 300; attempt += 1) {
    last = await evaluate(
      "document.querySelector('.connection-card')?.dataset.connectionState ?? 'loading'",
    );
    if (terminal.has(last)) return last;
    await delay(100);
  }
  throw new Error(`${label}: no terminal phase; got ${last}`);
}

async function activateButtonFromTrustedKeyboardGesture(send, evaluate, selector) {
  const focused = await evaluate(`(() => {
    const button = document.querySelector(${JSON.stringify(selector)});
    if (!(button instanceof HTMLButtonElement) || button.disabled) return false;
    button.focus({ preventScroll: false });
    return document.activeElement === button;
  })()`);
  if (!focused) throw new Error(`PRODUCTION_BUTTON_NOT_FOCUSABLE:${selector}`);
  const key = {
    key: "Enter",
    code: "Enter",
    text: "\r",
    unmodifiedText: "\r",
    windowsVirtualKeyCode: 13,
    nativeVirtualKeyCode: 13,
  };
  await send("Input.dispatchKeyEvent", { type: "keyDown", ...key });
  await send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: key.key,
    code: key.code,
    windowsVirtualKeyCode: key.windowsVirtualKeyCode,
    nativeVirtualKeyCode: key.nativeVirtualKeyCode,
  });
}

async function activateConnectionFromTrustedKeyboardGesture(send, evaluate) {
  await activateButtonFromTrustedKeyboardGesture(
    send,
    evaluate,
    ".connection-card > button",
  );
}

async function runScenario(browser, url, scenario, expectedPhase) {
  const profile = await mkdtemp(path.join(os.tmpdir(), `ade-webapp-readonly-${scenario}-`));
  const child = spawn(browser, [
    "--headless=new", "--disable-background-networking", "--disable-breakpad",
    "--disable-component-update", "--disable-default-apps", "--disable-dev-shm-usage",
    "--disable-extensions", "--disable-gpu", "--disable-sync", "--metrics-recording-only",
    "--no-default-browser-check", "--no-first-run", "--no-sandbox", "--remote-debugging-port=0",
    `--user-data-dir=${profile}`, "about:blank",
  ], { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  let endpoint;
  const devToolsUrl = new Promise((resolve, reject) => {
    child.stderr.setEncoding("utf8").on("data", (chunk) => {
      stderr += chunk;
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match && !endpoint) { endpoint = match[1]; resolve(endpoint); }
    });
    child.once("error", reject);
    child.once("close", (code) => { if (!endpoint) reject(new Error(`browser exited (${code})`)); });
  });

  let control;
  try {
    control = await devtoolsSocket(await devToolsUrl);
    await control.send("Page.enable");
    await control.send("Runtime.enable");
    await control.send("Page.bringToFront");
    await control.send("Page.addScriptToEvaluateOnNewDocument", { source: fakeSerialSource(scenario) });
    await control.send("Page.navigate", { url });
    await waitFor(
      control.evaluate,
      "document.querySelector('.connection-card')?.dataset.connectionState ?? 'loading'",
      "ready",
      `${scenario} preparation`,
    );
    await activateConnectionFromTrustedKeyboardGesture(control.send, control.evaluate);
    await delay(100);
    const gestureStarted = await control.evaluate(`(() => ({
      phase: document.querySelector('.connection-card')?.dataset.connectionState,
      requestCount: globalThis.__ADE_SERIAL_PROBE__?.serial?.requestCount ?? 0,
      focused: document.activeElement === document.querySelector('.connection-card button'),
    }))()`);
    if (gestureStarted.phase === "ready" && gestureStarted.requestCount === 0) {
      throw new Error(`PRODUCTION_TRUSTED_GESTURE_NOT_DELIVERED:${JSON.stringify(gestureStarted)}`);
    }
    const terminalPhase = await waitForTerminalPhase(control.evaluate, `${scenario} terminal phase`);
    if (terminalPhase !== expectedPhase) {
      const boundedDiagnostic = await control.evaluate(`(() => ({
        phase: document.querySelector('.connection-card')?.dataset.connectionState,
        fields: Object.fromEntries(
          [...document.querySelectorAll('[data-identity-field]')].map((node) => [
            node.dataset.identityField,
            node.querySelector('dd')?.textContent ?? '',
          ]),
        ),
        requestCount: globalThis.__ADE_SERIAL_PROBE__?.serial?.requestCount ?? 0,
        writeCount: globalThis.__ADE_SERIAL_PROBE__?.port?.writes?.length ?? 0,
        openCount: globalThis.__ADE_SERIAL_PROBE__?.port?.openCount ?? 0,
        closeCount: globalThis.__ADE_SERIAL_PROBE__?.port?.closeCount ?? 0,
      }))()`);
      throw new Error(
        `${scenario} terminal phase: expected ${expectedPhase}; ` +
          `bounded diagnostic ${JSON.stringify(boundedDiagnostic)}`,
      );
    }
    const diagnosticBefore = await control.evaluate(`(() => {
      const panel = document.querySelector('[data-diagnostic-trace="temporary"]');
      return {
        exists: panel instanceof HTMLDetailsElement,
        initiallyCollapsed: panel instanceof HTMLDetailsElement && !panel.open,
        traceCount: document.querySelectorAll('.diagnostic-trace-events li').length,
        traceLayers: [...document.querySelectorAll('[data-trace-layer]')].map((node) => node.dataset.traceLayer),
        traceEvents: [...document.querySelectorAll('.diagnostic-trace-events li')].map((node) => node.textContent ?? ''),
      };
    })()`);
    await control.evaluate(`(() => {
      const panel = document.querySelector('[data-diagnostic-trace="temporary"]');
      if (panel instanceof HTMLDetailsElement) panel.open = true;
    })()`);
    await activateButtonFromTrustedKeyboardGesture(
      control.send,
      control.evaluate,
      ".diagnostic-trace-actions button:first-of-type",
    );
    await delay(50);
    const copiedText = await control.evaluate(
      "globalThis.__ADE_SERIAL_PROBE__?.clipboardWrites?.at(-1) ?? ''",
    );
    await activateButtonFromTrustedKeyboardGesture(
      control.send,
      control.evaluate,
      ".diagnostic-trace-actions button:nth-of-type(2)",
    );
    await delay(50);
    const clearedCount = await control.evaluate(
      "document.querySelectorAll('.diagnostic-trace-events li').length",
    );
    return await control.evaluate(`(() => {
      const probe = globalThis.__ADE_SERIAL_PROBE__;
      const fields = Object.fromEntries(
        [...document.querySelectorAll('[data-identity-field]')].map((node) => [
          node.dataset.identityField,
          node.querySelector('dd')?.textContent ?? '',
        ]),
      );
      return {
        phase: document.querySelector('.connection-card')?.dataset.connectionState,
        text: document.querySelector('.connection-card')?.textContent ?? '',
        fields,
        requestCount: probe.serial.requestCount,
        userActivationAtRequest: probe.serial.userActivationAtRequest,
        writes: probe.port.writes,
        openCount: probe.port.openCount,
        closeCount: probe.port.closeCount,
        readerReleased: probe.port.readerReleased,
        writerReleased: probe.port.writerReleased,
        diagnostic: ${JSON.stringify(diagnosticBefore)},
        copiedText: ${JSON.stringify(copiedText)},
        clearedCount: ${JSON.stringify(clearedCount)},
      };
    })()`);
  } catch (error) {
    console.error(`PRODUCTION_SCENARIO_FAILURE:${scenario}:${stderr.slice(-3000)}`);
    throw error;
  } finally {
    control?.socket.close();
    child.kill();
    await Promise.race([new Promise((resolve) => child.once("close", resolve)), delay(5000)]);
    await rm(profile, { recursive: true, force: true });
  }
}

const browser = findBrowser();
if (!browser) {
  console.error("REAL_BROWSER_PRODUCTION_WEBAPP_ENVIRONMENT_UNAVAILABLE");
  process.exit(2);
}

const expectedRequests = [1, 2, 3, 4].map((command) => [36, 77, 60, 0, command, command]);
const prohibitedCommands = new Set([68, 99, 184, 185, 250]);
const server = serveProduction();
try {
  const port = await listen(server);
  const url = `http://127.0.0.1:${port}/`;
  const success = await runScenario(browser, url, "in-scope", "read-complete");
  if (
    success.requestCount !== 1 || !success.userActivationAtRequest ||
    JSON.stringify(success.writes) !== JSON.stringify(expectedRequests) ||
    success.openCount !== 1 || success.closeCount !== 1 ||
    success.readerReleased !== 1 || success.writerReleased !== 1 ||
    success.fields.apiVersion !== "1.46" || success.fields.fcVariant !== "BTFL" ||
    success.fields.fcVersion !== "4.5.5" || success.fields.targetName !== "SPEEDYBEEF405V4"
  ) throw new Error(`PRODUCTION_IN_SCOPE_PROOF_FAILED:${JSON.stringify(success)}`);
  if (
    !success.diagnostic.exists || !success.diagnostic.initiallyCollapsed ||
    success.diagnostic.traceCount === 0 || success.clearedCount !== 0 ||
    !success.copiedText.startsWith("FPV_ARBCON_READONLY_DIAGNOSTIC_TRACE_V1\n") ||
    !success.diagnostic.traceLayers.includes("RUST") ||
    !success.diagnostic.traceLayers.includes("MSP") ||
    !success.diagnostic.traceEvents.some((event) => event.includes("FINAL_OK")) ||
    /SPEEDYBEEF405V4|BTFL|36,77|usbVendorId|usbProductId/i.test(success.copiedText)
  ) throw new Error(`PRODUCTION_DIAGNOSTIC_PANEL_PROOF_FAILED:${JSON.stringify(success)}`);

  const mismatch = await runScenario(browser, url, "scope-mismatch", "scope-mismatch");
  if (
    JSON.stringify(mismatch.writes) !== JSON.stringify(expectedRequests) ||
    mismatch.fields.scopeMismatchField !== "msp_api_version" || mismatch.closeCount !== 1
  ) throw new Error(`PRODUCTION_SCOPE_MISMATCH_PROOF_FAILED:${JSON.stringify(mismatch)}`);

  const typedFailure = await runScenario(
    browser,
    url,
    "protocol-identity-failure",
    "failed",
  );
  if (
    JSON.stringify(typedFailure.writes) !== JSON.stringify(expectedRequests) ||
    typedFailure.fields.failure !== "ProtocolIdentityFailure" ||
    typedFailure.fields.failureStage !== "BOARD_INFO" ||
    typedFailure.fields.failureReason !== "TrailingPayload" ||
    typedFailure.fields.failureOrigin !== "IDENTITY_STAGE" ||
    typedFailure.closeCount !== 1
  ) throw new Error(`PRODUCTION_TYPED_DIAGNOSTIC_PROOF_FAILED:${JSON.stringify(typedFailure)}`);
  if (
    !typedFailure.diagnostic.traceEvents.some((event) => event.includes("IDENTITY_STAGE_FAILED")) ||
    !typedFailure.diagnostic.traceEvents.some((event) => event.includes("TrailingPayload"))
  ) throw new Error(`PRODUCTION_TYPED_TRACE_PROOF_FAILED:${JSON.stringify(typedFailure)}`);

  const cancelled = await runScenario(browser, url, "cancelled", "cancelled");
  if (cancelled.requestCount !== 1 || cancelled.writes.length !== 0 || cancelled.openCount !== 0) {
    throw new Error(`PRODUCTION_CANCELLED_PROOF_FAILED:${JSON.stringify(cancelled)}`);
  }
  const unavailable = await runScenario(browser, url, "unavailable", "unavailable");
  if (unavailable.requestCount !== 0 || unavailable.writes.length !== 0) {
    throw new Error(`PRODUCTION_UNAVAILABLE_PROOF_FAILED:${JSON.stringify(unavailable)}`);
  }

  for (const run of [success, mismatch, typedFailure, cancelled, unavailable]) {
    if (/hardwareObserved|serial.?number|usbVendorId|usbProductId|COM\d/i.test(run.text)) {
      throw new Error("PRODUCTION_UI_PRIVACY_BOUNDARY_FAILED");
    }
    if (run.writes.some((frame) => prohibitedCommands.has(frame[4]))) {
      throw new Error("PRODUCTION_UI_WRITE_AUTHORITY_FAILED");
    }
    if (
      !run.diagnostic.initiallyCollapsed || run.diagnostic.traceCount === 0 ||
      run.clearedCount !== 0 || !run.copiedText.includes("FINAL_")
    ) throw new Error("PRODUCTION_UI_DIAGNOSTIC_LIFECYCLE_FAILED");
    if (
      /COM99|SERIAL-SECRET-123|VID_1234|PID_ABCD|\/private\/path|raw-device-name/.test(
        `${run.text}\n${run.copiedText}\n${JSON.stringify(run.fields)}`,
      )
    ) throw new Error("PRODUCTION_UI_DIAGNOSTIC_PRIVACY_ATTACK_FAILED");
  }
  for (const asset of [
    "/wasm/ade_web_readonly_serial_wasm_bridge.js",
    "/wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
  ]) {
    if (!servedRequests.includes(asset)) throw new Error(`PRODUCTION_VERIFIED_ASSET_NOT_LOADED:${asset}`);
  }
  console.log("production Smart Configurator Rust WASM read-only connection passed");
  console.log("SOFTWARE_EXERCISED;REAL_CHROME_EXERCISED;PHYSICAL_FC_NOT_TESTED;HARDWARE_OBSERVED=NO");
} finally {
  await new Promise((resolve) => server.close(resolve));
}
