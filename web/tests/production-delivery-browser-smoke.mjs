import fs from "node:fs";
import http from "node:http";
import { mkdtemp, rm } from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));

function normalizedBasePath(value = "/") {
  const candidate = value.trim();
  if (!candidate.startsWith("/") || /[?#]/.test(candidate)) {
    throw new Error("BASE_PATH_MUST_BE_ABSOLUTE");
  }
  const withoutTrailingSlash = candidate.replace(/\/+$/, "");
  return withoutTrailingSlash ? `${withoutTrailingSlash}/` : "/";
}

function requireSha(value, label) {
  if (!/^[0-9a-f]{7,64}$/.test(value ?? "")) throw new Error(`${label}_MUST_BE_A_SHA`);
  return value;
}

const currentDist = path.resolve(process.argv[2] ?? path.join(webRoot, "dist"));
const basePath = normalizedBasePath(process.argv[3]);
const currentSha = requireSha(process.argv[4], "CURRENT_SHA");
const previousDist = process.argv[5] ? path.resolve(process.argv[5]) : null;
const previousSha = previousDist ? requireSha(process.argv[6], "PREVIOUS_SHA") : null;

for (const directory of [currentDist, previousDist].filter(Boolean)) {
  for (const required of [
    "index.html",
    "manifest.webmanifest",
    "sw.js",
    "wasm/ade_web_storage_wasm_bridge.js",
    "wasm/ade_web_storage_wasm_bridge_bg.wasm",
    "wasm/ade_web_readonly_serial_wasm_bridge.js",
    "wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
  ]) {
    if (!fs.statSync(path.join(directory, required), { throwIfNoEntry: false })?.isFile()) {
      throw new Error(`PRODUCTION_ARTIFACT_MISSING:${directory}:${required}`);
    }
  }
}

const contentTypes = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".svg", "image/svg+xml"],
  [".wasm", "application/wasm"],
  [".webmanifest", "application/manifest+json; charset=utf-8"],
]);

function productionServer(initialDist) {
  const state = {
    dist: initialDist,
    offline: false,
    requests: [],
  };
  const server = http.createServer((request, response) => {
    const requestUrl = new URL(request.url, "http://127.0.0.1");
    state.requests.push({
      pathname: requestUrl.pathname,
      offline: state.offline,
      version: state.dist === currentDist ? "current" : "previous",
    });
    if (state.offline) {
      request.socket.destroy();
      return;
    }
    if (!requestUrl.pathname.startsWith(basePath)) {
      response.writeHead(404, { "Cache-Control": "no-store" });
      response.end();
      return;
    }
    const relative = decodeURIComponent(requestUrl.pathname.slice(basePath.length));
    const requested = relative === "" ? "index.html" : relative;
    const candidate = path.resolve(state.dist, requested);
    const contained = candidate === state.dist || candidate.startsWith(`${state.dist}${path.sep}`);
    if (!contained || !fs.statSync(candidate, { throwIfNoEntry: false })?.isFile()) {
      response.writeHead(404, { "Cache-Control": "no-store" });
      response.end();
      return;
    }
    const payload = fs.readFileSync(candidate);
    response.writeHead(200, {
      "Content-Type": contentTypes.get(path.extname(candidate)) ?? "application/octet-stream",
      "Content-Length": payload.length,
      "Cache-Control": "no-cache",
      "Cross-Origin-Resource-Policy": "same-origin",
      "X-Content-Type-Options": "nosniff",
    });
    response.end(payload);
  });
  return { server, state };
}

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
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

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function connectToPage(browserEndpoint) {
  const endpoint = new URL(browserEndpoint);
  let target;
  for (let attempt = 0; attempt < 100 && !target; attempt += 1) {
    try {
      const targets = await fetch(`http://${endpoint.host}/json/list`).then((response) => response.json());
      target = targets.find((candidate) => candidate.type === "page");
    } catch {
      // The target can appear shortly after the browser endpoint.
    }
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
    if (response.exceptionDetails) {
      throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text);
    }
    return response.result.value;
  };
  return { socket, send, evaluate };
}

async function launchBrowser(browser, profile) {
  const child = spawn(browser, [
    "--headless=new", "--disable-background-networking", "--disable-breakpad",
    "--disable-component-update", "--disable-default-apps", "--disable-dev-shm-usage",
    "--disable-extensions", "--disable-gpu", "--disable-sync", "--metrics-recording-only",
    "--no-default-browser-check", "--no-first-run", "--no-sandbox", "--remote-debugging-port=0",
    `--user-data-dir=${profile}`, "about:blank",
  ], { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  let endpoint;
  const browserEndpoint = new Promise((resolve, reject) => {
    child.stderr.setEncoding("utf8").on("data", (chunk) => {
      stderr += chunk;
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match && !endpoint) {
        endpoint = match[1];
        resolve(endpoint);
      }
    });
    child.once("error", reject);
    child.once("close", (code) => {
      if (!endpoint) reject(new Error(`browser exited before DevTools (${code})`));
    });
  });
  return { child, stderr: () => stderr, control: await connectToPage(await browserEndpoint) };
}

async function waitForBuild(evaluate, expectedSha) {
  for (let attempt = 0; attempt < 300; attempt += 1) {
    try {
      const observed = await evaluate(`document.documentElement?.dataset.buildSha ?? ""`);
      const initialized = await evaluate(`document.querySelector("#root")?.childElementCount > 0`);
      if (observed === expectedSha && initialized) return;
    } catch {
      // Navigation can briefly replace the execution context before the new document exists.
    }
    await delay(100);
  }
  throw new Error(`PRODUCTION_APP_INITIALIZATION_TIMEOUT:${expectedSha}`);
}

async function waitForVersionedController(evaluate, expectedSha) {
  let state = null;
  for (let attempt = 0; attempt < 200; attempt += 1) {
    state = await evaluate(`navigator.serviceWorker?.getRegistration().then((registration) => ({
      active: registration?.active?.state === "activated"
        ? new URL(registration.active.scriptURL).searchParams.get("version")
        : null,
      controller: navigator.serviceWorker.controller
        ? new URL(navigator.serviceWorker.controller.scriptURL).searchParams.get("version")
        : null,
      pwaStatus: document.documentElement.dataset.pwaStatus ?? null,
    }))`);
    if (state?.active === expectedSha && state.controller === expectedSha) {
      return;
    }
    await delay(100);
  }
  throw new Error(
    `PRODUCTION_VERSIONED_SERVICE_WORKER_DID_NOT_TAKE_CONTROL:${expectedSha}:${JSON.stringify(state)}`,
  );
}

async function waitForCachedAssetSet(evaluate, cacheName) {
  let cachedUrls = [];
  for (let attempt = 0; attempt < 100; attempt += 1) {
    cachedUrls = await evaluate(`caches.open(${JSON.stringify(cacheName)})
      .then((cache) => cache.keys())
      .then((requests) => requests.map((request) => request.url))`);
    const hasScript = cachedUrls.some((value) => /\/assets\/[^/]+\.js$/.test(new URL(value).pathname));
    const hasStyle = cachedUrls.some((value) => /\/assets\/[^/]+\.css$/.test(new URL(value).pathname));
    if (hasScript && hasStyle) return cachedUrls;
    await delay(100);
  }
  const workerState = await evaluate(`navigator.serviceWorker.getRegistration().then((registration) => ({
    active: registration?.active?.scriptURL ?? null,
    activeState: registration?.active?.state ?? null,
    controller: navigator.serviceWorker.controller?.scriptURL ?? null
  }))`);
  const resourceState = await evaluate(`({
    performance: performance.getEntriesByType("resource").map((entry) => entry.name),
    document: [...document.querySelectorAll("script[src], link[href]")].map((element) => element.src || element.href)
  })`);
  throw new Error(
    `SERVICE_WORKER_ASSET_SET_INCOMPLETE:${cacheName}:${JSON.stringify({ cachedUrls, workerState, resourceState })}`,
  );
}

async function inspect(evaluate) {
  return evaluate(`(async () => {
    const base = new URL(${JSON.stringify(basePath)}, location.origin);
    const manifestResponse = await fetch(new URL("manifest.webmanifest", base));
    if (!manifestResponse.ok) throw new Error("MANIFEST_FETCH_FAILED");
    const manifest = await manifestResponse.json();
    const registration = await navigator.serviceWorker.ready;
    const storage = await import(new URL("wasm/ade_web_storage_wasm_bridge.js", base).href);
    await storage.default({ module_or_path: new URL("wasm/ade_web_storage_wasm_bridge_bg.wasm", base) });
    const serial = await import(new URL("wasm/ade_web_readonly_serial_wasm_bridge.js", base).href);
    await serial.default({ module_or_path: new URL("wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm", base) });
    return {
      buildSha: document.documentElement.dataset.buildSha,
      rootReady: document.querySelector("#root")?.childElementCount > 0,
      manifestUrl: manifestResponse.url,
      manifestStart: new URL(manifest.start_url, manifestResponse.url).href,
      manifestScope: new URL(manifest.scope, manifestResponse.url).href,
      iconUrl: new URL(manifest.icons[0].src, manifestResponse.url).href,
      workerScope: registration.scope,
      resources: performance.getEntriesByType("resource").map((entry) => entry.name),
      caches: await caches.keys(),
    };
  })()`);
}

function requireInspection(result, origin, expectedSha) {
  const expectedBase = `${origin}${basePath}`;
  if (!result.rootReady || result.buildSha !== expectedSha) throw new Error("APP_NOT_INITIALIZED");
  for (const value of [
    result.manifestUrl,
    result.manifestStart,
    result.manifestScope,
    result.iconUrl,
    result.workerScope,
  ]) {
    if (!value.startsWith(expectedBase)) throw new Error(`PWA_URL_ESCAPED_SCOPE:${value}`);
  }
  if (!result.resources.some((value) => /\/assets\/[^/]+\.js$/.test(new URL(value).pathname))) {
    throw new Error("BUILT_JAVASCRIPT_NOT_LOADED");
  }
  if (!result.resources.some((value) => /\/assets\/[^/]+\.css$/.test(new URL(value).pathname))) {
    throw new Error("BUILT_CSS_NOT_LOADED");
  }
}

const browser = findBrowser();
if (!browser) {
  console.error("REAL_BROWSER_PRODUCTION_DELIVERY_ENVIRONMENT_UNAVAILABLE");
  process.exit(2);
}

const profile = await mkdtemp(path.join(os.tmpdir(), "ade-production-delivery-"));
const served = productionServer(previousDist ?? currentDist);
let launched;
try {
  const port = await listen(served.server);
  const origin = `http://127.0.0.1:${port}`;
  const pageUrl = `${origin}${basePath}`;
  launched = await launchBrowser(browser, profile);
  await launched.control.send("Page.enable");
  await launched.control.send("Runtime.enable");
  await launched.control.send("Page.navigate", { url: pageUrl });

  const initialSha = previousSha ?? currentSha;
  await waitForBuild(launched.control.evaluate, initialSha);
  await waitForVersionedController(launched.control.evaluate, initialSha);
  const online = await inspect(launched.control.evaluate);
  requireInspection(online, origin, initialSha);
  await waitForCachedAssetSet(
    launched.control.evaluate,
    `smart-configurator-shell-${initialSha}`,
  );

  served.state.offline = true;
  await launched.control.send("Page.reload", { ignoreCache: true });
  await waitForBuild(launched.control.evaluate, initialSha);
  const offline = await inspect(launched.control.evaluate);
  requireInspection(offline, origin, initialSha);
  served.state.offline = false;

  let updated = null;
  let updatedOffline = null;
  if (previousDist) {
    served.state.dist = currentDist;
    await launched.control.send("Page.navigate", { url: `${pageUrl}?commit=${currentSha}` });
    await waitForBuild(launched.control.evaluate, currentSha);
    await waitForVersionedController(launched.control.evaluate, currentSha);
    updated = await inspect(launched.control.evaluate);
    requireInspection(updated, origin, currentSha);
    for (let attempt = 0; attempt < 100; attempt += 1) {
      updated.caches = await launched.control.evaluate("caches.keys()");
      if (!updated.caches.some((name) => name.endsWith(previousSha))) break;
      await delay(100);
    }
    if (updated.caches.some((name) => name.endsWith(previousSha))) {
      throw new Error("OBSOLETE_SERVICE_WORKER_CACHE_RETAINED");
    }

    await waitForCachedAssetSet(
      launched.control.evaluate,
      `smart-configurator-shell-${currentSha}`,
    );

    served.state.offline = true;
    await launched.control.send("Page.reload", { ignoreCache: true });
    await waitForBuild(launched.control.evaluate, currentSha);
    updatedOffline = await inspect(launched.control.evaluate);
    requireInspection(updatedOffline, origin, currentSha);
    served.state.offline = false;
  }

  const escapedWasm = served.state.requests.filter((entry) => entry.pathname.startsWith("/wasm/"));
  if (basePath !== "/" && escapedWasm.length > 0) {
    throw new Error(`WASM_REQUEST_ESCAPED_REPOSITORY_SCOPE:${JSON.stringify(escapedWasm)}`);
  }
  for (const required of [
    "manifest.webmanifest",
    "sw.js",
    "wasm/ade_web_storage_wasm_bridge.js",
    "wasm/ade_web_storage_wasm_bridge_bg.wasm",
    "wasm/ade_web_readonly_serial_wasm_bridge.js",
    "wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
  ]) {
    if (!served.state.requests.some((entry) => entry.pathname === `${basePath}${required}`)) {
      throw new Error(`PRODUCTION_REQUEST_MISSING:${required}`);
    }
  }

  console.log(JSON.stringify({
    result: "PRODUCTION_DELIVERY_BROWSER_PASS",
    basePath,
    onlineSha: online.buildSha,
    offlineSha: offline.buildSha,
    updatedSha: updated?.buildSha ?? null,
    updatedOfflineSha: updatedOffline?.buildSha ?? null,
    workerScope: (updated ?? online).workerScope,
    cacheNames: (updated ?? online).caches,
  }));
} catch (error) {
  console.error(launched?.stderr().slice(-4000));
  throw error;
} finally {
  launched?.control.socket.close();
  launched?.child.kill();
  if (launched) {
    await Promise.race([new Promise((resolve) => launched.child.once("close", resolve)), delay(5000)]);
  }
  await new Promise((resolve) => served.server.close(resolve));
  await rm(profile, { recursive: true, force: true });
}
