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
    [36, 77, 62, 88, 4, 83, 52, 48, 53, 0, 0, 0, 0, 15, 83, 80, 69, 69, 68, 89, 66, 69, 69, 70, 52, 48, 53, 86, 52, 17, 83, 112, 101, 101, 100, 121, 66, 101, 101, 32, 70, 52, 48, 53, 32, 86, 52, 3, 83, 80, 66, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 66],
  ].map((bytes) => Uint8Array.from(bytes));
  const replies = scenario === "scope-mismatch"
    ? [Uint8Array.from([36, 77, 62, 3, 1, 0, 1, 45, 46]), ...inScopeReplies.slice(1)]
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
        const error = new Error("owner cancelled");
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
  globalThis.__ADE_SERIAL_PROBE__ = { scenario, serial, port };
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

async function activateConnectionFromTrustedKeyboardGesture(send, evaluate) {
  const focused = await evaluate(`(() => {
    const button = document.querySelector('.connection-card button');
    if (!(button instanceof HTMLButtonElement) || button.disabled) return false;
    button.focus({ preventScroll: false });
    return document.activeElement === button;
  })()`);
  if (!focused) throw new Error("PRODUCTION_CONNECTION_BUTTON_NOT_FOCUSABLE");
  const key = { key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13 };
  await send("Input.dispatchKeyEvent", { type: "keyDown", ...key });
  await send("Input.dispatchKeyEvent", { type: "keyUp", ...key });
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
    await control.send("Page.addScriptToEvaluateOnNewDocument", { source: fakeSerialSource(scenario) });
    await control.send("Page.navigate", { url });
    await waitFor(
      control.evaluate,
      "document.querySelector('.connection-card')?.dataset.connectionState ?? 'loading'",
      "ready",
      `${scenario} preparation`,
    );
    await activateConnectionFromTrustedKeyboardGesture(control.send, control.evaluate);
    await waitFor(
      control.evaluate,
      "document.querySelector('.connection-card')?.dataset.connectionState ?? 'loading'",
      expectedPhase,
      `${scenario} terminal phase`,
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

  const mismatch = await runScenario(browser, url, "scope-mismatch", "scope-mismatch");
  if (
    JSON.stringify(mismatch.writes) !== JSON.stringify(expectedRequests) ||
    mismatch.fields.scopeMismatchField !== "msp_api_version" || mismatch.closeCount !== 1
  ) throw new Error(`PRODUCTION_SCOPE_MISMATCH_PROOF_FAILED:${JSON.stringify(mismatch)}`);

  const cancelled = await runScenario(browser, url, "cancelled", "cancelled");
  if (cancelled.requestCount !== 1 || cancelled.writes.length !== 0 || cancelled.openCount !== 0) {
    throw new Error(`PRODUCTION_CANCELLED_PROOF_FAILED:${JSON.stringify(cancelled)}`);
  }
  const unavailable = await runScenario(browser, url, "unavailable", "unavailable");
  if (unavailable.requestCount !== 0 || unavailable.writes.length !== 0) {
    throw new Error(`PRODUCTION_UNAVAILABLE_PROOF_FAILED:${JSON.stringify(unavailable)}`);
  }

  for (const run of [success, mismatch, cancelled, unavailable]) {
    if (/hardwareObserved|serial.?number|usbVendorId|usbProductId|COM\d/i.test(run.text)) {
      throw new Error("PRODUCTION_UI_PRIVACY_BOUNDARY_FAILED");
    }
    if (run.writes.some((frame) => prohibitedCommands.has(frame[4]))) {
      throw new Error("PRODUCTION_UI_WRITE_AUTHORITY_FAILED");
    }
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
