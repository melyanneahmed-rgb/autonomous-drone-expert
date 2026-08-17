import fs from "node:fs";
import http from "node:http";
import { mkdtemp, rm } from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const browserRoot = path.join(webRoot, "tests", "browser");
const adapterPath = path.join(webRoot, "src", "storage", "indexeddb-journal-store.ts");
const contractPath = path.join(webRoot, "src", "storage", "journal-storage-contract.mjs");
const servedRequests = [];

function normalizedBasePath(value = "/") {
  const candidate = value.trim();
  if (!candidate.startsWith("/") || /[?#]/.test(candidate)) {
    throw new Error("BASE_PATH_MUST_BE_ABSOLUTE");
  }
  const withoutTrailingSlash = candidate.replace(/\/+$/, "");
  return withoutTrailingSlash ? `${withoutTrailingSlash}/` : "/";
}

const basePath = normalizedBasePath(process.argv[2]);
const route = (relative = "") => `${basePath}${relative}`;

async function transpileAdapter() {
  const source = fs.readFileSync(adapterPath, "utf8");
  try {
    const typescript = await import("typescript");
    return typescript.default.transpileModule(source, {
      compilerOptions: {
        module: typescript.default.ModuleKind.ES2022,
        target: typescript.default.ScriptTarget.ES2022,
        useDefineForClassFields: true,
      },
    }).outputText;
  } catch {
    const { stripTypeScriptTypes } = await import("node:module");
    return stripTypeScriptTypes(source, { mode: "strip", sourceMap: false });
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

function serve(adapterSource) {
  const routes = new Map([
    [route(), ["text/html; charset=utf-8", fs.readFileSync(path.join(browserRoot, "indexeddb-smoke.html"))]],
    [route("indexeddb-smoke.mjs"), ["text/javascript; charset=utf-8", fs.readFileSync(path.join(browserRoot, "indexeddb-smoke.mjs"))]],
    [route("adapter.js"), ["text/javascript; charset=utf-8", Buffer.from(adapterSource)]],
    [route("journal-storage-contract.mjs"), ["text/javascript; charset=utf-8", fs.readFileSync(contractPath)]],
  ]);
  return http.createServer((request, response) => {
    servedRequests.push(request.url);
    const selectedRoute = routes.get(request.url);
    if (!selectedRoute) {
      response.writeHead(request.url === route("favicon.ico") ? 204 : 404);
      response.end();
      return;
    }
    response.writeHead(200, {
      "Content-Type": selectedRoute[0],
      "Cache-Control": "no-store",
      "Cross-Origin-Resource-Policy": "same-origin",
    });
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

async function findPageTarget(devToolsUrl, pageUrl) {
  const endpoint = new URL(devToolsUrl);
  const listUrl = `http://${endpoint.host}/json/list`;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const targets = await fetch(listUrl).then((response) => response.json());
      const target = targets.find(
        (candidate) => candidate.type === "page" && candidate.url === pageUrl,
      );
      if (target?.webSocketDebuggerUrl) return target;
    } catch {
      // The DevTools endpoint can precede target registration by a few milliseconds.
    }
    await delay(50);
  }
  throw new Error("REAL_BROWSER_INDEXEDDB_TARGET_UNAVAILABLE");
}

async function inspectPage(devToolsUrl, pageUrl) {
  const target = await findPageTarget(devToolsUrl, pageUrl);
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  let commandId = 0;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    if (message.error) waiter.reject(new Error(message.error.message));
    else waiter.resolve(message.result.result.value);
  });

  const evaluate = (expression) =>
    new Promise((resolve, reject) => {
      commandId += 1;
      pending.set(commandId, { resolve, reject });
      socket.send(
        JSON.stringify({
          id: commandId,
          method: "Runtime.evaluate",
          params: { expression, returnByValue: true },
        }),
      );
    });

  try {
    for (let attempt = 0; attempt < 200; attempt += 1) {
      const state = await evaluate("document.body?.dataset.result ?? 'loading'");
      if (state === "pass" || state === "fail") {
        const output = await evaluate("document.querySelector('#result')?.textContent ?? ''");
        return { state, output };
      }
      await delay(100);
    }
    throw new Error("REAL_BROWSER_INDEXEDDB_TEST_TIMEOUT");
  } finally {
    socket.close();
  }
}

async function runBrowser(browser, url, profile) {
  const child = spawn(browser, [
    "--headless=new",
    "--disable-background-networking",
    "--disable-breakpad",
    "--disable-component-update",
    "--disable-default-apps",
    "--disable-dev-shm-usage",
    "--disable-extensions",
    "--disable-gpu",
    "--disable-sync",
    "--metrics-recording-only",
    "--no-default-browser-check",
    "--no-first-run",
    "--no-sandbox",
    "--remote-debugging-port=0",
    `--user-data-dir=${profile}`,
    url,
  ], { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  let devToolsUrl;
  let resolveEndpoint;
  let rejectEndpoint;
  const endpoint = new Promise((resolve, reject) => {
    resolveEndpoint = resolve;
    rejectEndpoint = reject;
  });
  child.stderr.setEncoding("utf8").on("data", (chunk) => {
    stderr += chunk;
    const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
    if (match && !devToolsUrl) {
      devToolsUrl = match[1];
      resolveEndpoint(devToolsUrl);
    }
  });
  child.once("error", rejectEndpoint);
  child.once("close", (code) => {
    if (!devToolsUrl) rejectEndpoint(new Error(`browser exited before DevTools (${code})`));
  });

  try {
    const result = await inspectPage(await endpoint, url);
    return { ...result, stderr };
  } finally {
    child.kill();
    await Promise.race([
      new Promise((resolve) => child.once("close", resolve)),
      delay(5000),
    ]);
  }
}

const browser = findBrowser();
if (!browser) {
  console.error("REAL_BROWSER_INDEXEDDB_TEST_ENVIRONMENT_UNAVAILABLE");
  process.exit(2);
}

const profile = await mkdtemp(path.join(os.tmpdir(), "ade-indexeddb-smoke-"));
const server = serve(await transpileAdapter());
try {
  const port = await listen(server);
  const result = await runBrowser(browser, `http://127.0.0.1:${port}${basePath}`, profile);
  if (
    result.state !== "pass" ||
    result.output !== "INDEXEDDB_BROWSER_SMOKE_PASS"
  ) {
    console.error(`REAL_BROWSER_INDEXEDDB_TEST_FAILED (${result.output})`);
    console.error(`browser requests: ${servedRequests.join(", ")}`);
    console.error(result.stderr.slice(-4000));
    process.exitCode = 1;
  } else {
    console.log("real-browser IndexedDB journal CAS smoke passed");
  }
} finally {
  await new Promise((resolve) => server.close(resolve));
  await rm(profile, { recursive: true, force: true });
}
