import fs from "node:fs";
import http from "node:http";
import { mkdtemp, rm } from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const browserRoot = path.join(webRoot, "tests", "browser");
const hostPath = path.join(webRoot, "src", "transport", "webserial-readonly-host.mjs");
const diagnosticPath = path.join(webRoot, "src", "diagnostics", "readonly-trace.mjs");
const glueRoot = path.resolve(
  process.argv[2] ?? path.join(path.dirname(webRoot), "target", "webserial-wasm-web"),
);
const glueJs = path.join(glueRoot, "ade_web_readonly_serial_wasm_bridge.js");
const glueWasm = path.join(glueRoot, "ade_web_readonly_serial_wasm_bridge_bg.wasm");
const servedRequests = [];

function normalizedBasePath(value = "/") {
  const candidate = value.trim();
  if (!candidate.startsWith("/") || /[?#]/.test(candidate)) {
    throw new Error("BASE_PATH_MUST_BE_ABSOLUTE");
  }
  const withoutTrailingSlash = candidate.replace(/\/+$/, "");
  return withoutTrailingSlash ? `${withoutTrailingSlash}/` : "/";
}

const basePath = normalizedBasePath(process.argv[3]);
const route = (relative = "") => `${basePath}${relative}`;

for (const required of [glueJs, glueWasm]) {
  if (!fs.existsSync(required)) {
    console.error(`WEB_SERIAL_WASM_GLUE_MISSING:${required}`);
    process.exit(2);
  }
}

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

function serve() {
  const routes = new Map([
    [route(), ["text/html; charset=utf-8", fs.readFileSync(path.join(browserRoot, "webserial-readonly-smoke.html"))]],
    [route("webserial-readonly-smoke.mjs"), ["text/javascript; charset=utf-8", fs.readFileSync(path.join(browserRoot, "webserial-readonly-smoke.mjs"))]],
    [route("transport/webserial-readonly-host.mjs"), ["text/javascript; charset=utf-8", fs.readFileSync(hostPath)]],
    [route("diagnostics/readonly-trace.mjs"), ["text/javascript; charset=utf-8", fs.readFileSync(diagnosticPath)]],
    [route("wasm/ade_web_readonly_serial_wasm_bridge.js"), ["text/javascript; charset=utf-8", fs.readFileSync(glueJs)]],
    [route("wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm"), ["application/wasm", fs.readFileSync(glueWasm)]],
  ]);
  return http.createServer((request, response) => {
    servedRequests.push(request.url);
    const selectedRoute = routes.get(request.url);
    if (!selectedRoute) {
      response.writeHead(request.url === route("favicon.ico") ? 204 : 404);
      response.end();
      return;
    }
    response.writeHead(200, { "Content-Type": selectedRoute[0], "Cache-Control": "no-store" });
    response.end(selectedRoute[1]);
  });
}

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function inspectPage(devToolsUrl, pageUrl) {
  const endpoint = new URL(devToolsUrl);
  let target;
  for (let attempt = 0; attempt < 100 && !target; attempt += 1) {
    try {
      const targets = await fetch(`http://${endpoint.host}/json/list`).then((response) => response.json());
      target = targets.find((candidate) => candidate.type === "page" && candidate.url === pageUrl);
    } catch {}
    if (!target) await delay(50);
  }
  if (!target?.webSocketDebuggerUrl) throw new Error("WEB_SERIAL_BROWSER_TARGET_UNAVAILABLE");
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
    else waiter.resolve(message.result.result.value);
  });
  const evaluate = (expression) => new Promise((resolve, reject) => {
    id += 1;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
  });
  try {
    for (let attempt = 0; attempt < 300; attempt += 1) {
      const state = await evaluate("document.body?.dataset.result ?? 'loading'");
      if (state === "pass" || state === "fail") {
        return { state, output: await evaluate("document.querySelector('#result')?.textContent ?? ''") };
      }
      await delay(100);
    }
    const output = await evaluate("document.querySelector('#result')?.textContent ?? ''");
    throw new Error(`WEB_SERIAL_BROWSER_TEST_TIMEOUT:${output}`);
  } finally {
    socket.close();
  }
}

async function runBrowser(browser, url, profile) {
  const child = spawn(browser, [
    "--headless=new", "--disable-background-networking", "--disable-breakpad",
    "--disable-component-update", "--disable-default-apps", "--disable-dev-shm-usage",
    "--disable-extensions", "--disable-gpu", "--disable-sync", "--metrics-recording-only",
    "--no-default-browser-check", "--no-first-run", "--no-sandbox", "--remote-debugging-port=0",
    `--user-data-dir=${profile}`, url,
  ], { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  let devToolsUrl;
  const endpoint = new Promise((resolve, reject) => {
    child.stderr.setEncoding("utf8").on("data", (chunk) => {
      stderr += chunk;
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match && !devToolsUrl) { devToolsUrl = match[1]; resolve(devToolsUrl); }
    });
    child.once("error", reject);
    child.once("close", (code) => { if (!devToolsUrl) reject(new Error(`browser exited (${code})`)); });
  });
  try {
    return { ...(await inspectPage(await endpoint, url)), stderr };
  } finally {
    child.kill();
    await Promise.race([new Promise((resolve) => child.once("close", resolve)), delay(5000)]);
  }
}

const browser = findBrowser();
if (!browser) {
  console.error("REAL_BROWSER_WEB_SERIAL_ENVIRONMENT_UNAVAILABLE");
  process.exit(2);
}

const profile = await mkdtemp(path.join(os.tmpdir(), "ade-webserial-readonly-smoke-"));
const server = serve();
try {
  const port = await listen(server);
  const result = await runBrowser(browser, `http://127.0.0.1:${port}${basePath}`, profile);
  const fetched = servedRequests.includes(route("wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm"));
  if (result.state !== "pass" || result.output !== "WEB_SERIAL_READONLY_BROWSER_PASS:A+B+C+D+E+F+G+H+I+J" || !fetched) {
    console.error(`REAL_BROWSER_WEB_SERIAL_FAILED (${result.output})`);
    console.error(result.stderr.slice(-4000));
    process.exitCode = 1;
  } else {
    console.log("real-browser Rust WASM + Web Serial read-only gate passed (A-J)");
  }
} catch (error) {
  console.error(`WEB_SERIAL_BROWSER_REQUESTS:${servedRequests.join(",")}`);
  throw error;
} finally {
  await new Promise((resolve) => server.close(resolve));
  await rm(profile, { recursive: true, force: true });
}
