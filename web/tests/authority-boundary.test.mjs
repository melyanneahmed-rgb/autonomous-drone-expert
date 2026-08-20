import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
function sourceFiles(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(directory, entry.name);
    return entry.isDirectory() ? sourceFiles(full) : [full];
  });
}

test("product surface keeps storage and read-only serial authority in designated adapters", () => {
  const files = [path.join(webRoot, "index.html"), ...sourceFiles(path.join(webRoot, "src")), ...sourceFiles(path.join(webRoot, "public"))]
    .filter(
      (file) =>
        !file.endsWith("favicon.svg") &&
        !file.startsWith(path.join(webRoot, "public", "wasm", path.sep)),
    );
  const source = files.map((file) => fs.readFileSync(file, "utf8")).join("\n");
  const forbidden = [
    /navigator\s*\?*\.\s*(usb|hid|bluetooth)/i,
    /\b(USBDevice|HIDDevice|BluetoothDevice)\b/,
    /\b(localStorage|sessionStorage)\b/,
    /\b(WebSocket|EventSource|sendBeacon|XMLHttpRequest)\b/,
    /\b(analytics|telemetry)\b/i,
    /\b(WriteApproval|motor command|buildMspFrame|flashFirmware|dfuUtil)\b/i,
  ];
  for (const pattern of forbidden) assert.doesNotMatch(source, pattern);

  const wasmOwners = files
    .filter((file) => /\.wasm\b|WebAssembly/.test(fs.readFileSync(file, "utf8")))
    .map((file) => path.relative(webRoot, file).replaceAll("\\", "/"));
  assert.deepEqual(wasmOwners, ["src/pwa-register.ts"]);

  const pwaRegistration = fs.readFileSync(path.join(webRoot, "src", "pwa-register.ts"), "utf8");
  assert.match(pwaRegistration, /OFFLINE_RUNTIME_ASSET_PATHS/);
  assert.match(pwaRegistration, /new URL\(relativePath, baseUrl\)\.href/);
  assert.doesNotMatch(pwaRegistration, /["']\/wasm\//);

  const connection = fs.readFileSync(
    path.join(webRoot, "src", "connection", "readonly-fc-connection.mjs"),
    "utf8",
  );
  assert.match(connection, /from "virtual:ade-web-readonly-serial-wasm"/);
  assert.match(connection, /await initReadonlySerialWasm\(\)/);
  assert.doesNotMatch(connection, /["']\/wasm\//);

  const worker = fs.readFileSync(path.join(webRoot, "public", "sw.js"), "utf8");
  assert.match(worker, /relativePath\.startsWith\("wasm\/"\)/);

  const adapter = path.join(webRoot, "src", "storage", "indexeddb-journal-store.ts");
  const outsideAdapter = files
    .filter((file) => file !== adapter)
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  assert.doesNotMatch(outsideAdapter, /\bindexedDB\b/);
  assert.match(fs.readFileSync(adapter, "utf8"), /globalThis\.indexedDB/);

  const serialAdapter = path.join(webRoot, "src", "transport", "webserial-readonly-host.mjs");
  const serialDeclaration = path.join(
    webRoot,
    "src",
    "transport",
    "webserial-readonly-host.d.mts",
  );
  const outsideSerialAdapter = files
    .filter((file) => file !== serialAdapter)
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  assert.doesNotMatch(outsideSerialAdapter, /navigator\s*\?*\.\s*serial|\brequestPort\b/);
  const serialSource = fs.readFileSync(serialAdapter, "utf8");
  const serialTypes = fs.readFileSync(serialDeclaration, "utf8");
  assert.match(serialSource, /globalThis\.navigator\?\.serial/);
  assert.match(serialSource, /from "virtual:ade-web-readonly-serial-wasm"/);
  const vite = fs.readFileSync(path.join(webRoot, "vite.config.ts"), "utf8");
  assert.match(vite, /ADE_WEB_BASE_PATH/);
  assert.match(vite, /virtual:ade-web-readonly-serial-wasm/);
  assert.match(vite, /readonlyWasmGlue = `\$\{base\}wasm\/ade_web_readonly_serial_wasm_bridge\.js`/);
  assert.match(vite, /readonlyWasmBinary = `\$\{base\}wasm\/ade_web_readonly_serial_wasm_bridge_bg\.wasm`/);
  assert.match(vite, /module_or_path: new URL/);
  assert.match(serialSource, /new WasmReadonlySerialDiscovery\(\)/);
  assert.match(serialSource, /instanceof WasmReadonlySerialDirective/);
  assert.match(serialSource, /async discover\(\)/);
  assert.doesNotMatch(
    serialSource,
    /rustDirectiveType|setRustBindings|\bbindings\b|bindingFactory|discoveryFactory|directiveFactory|trustCallback/,
  );
  assert.match(serialTypes, /constructor\(options\?: \{ serial\?: object; timeoutMs\?: number \}\)/);
  assert.match(serialTypes, /discover\(\): Promise<ReadonlyDiscoveryResult>/);
  assert.doesNotMatch(serialTypes, /rustDirectiveType|discover\(discovery|\bbindings\b|bindingFactory|validator/);
});

test("the only network primitive is the guarded same-origin service worker fetch", () => {
  const appSource = sourceFiles(path.join(webRoot, "src"))
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  const worker = fs.readFileSync(path.join(webRoot, "public", "sw.js"), "utf8");
  assert.doesNotMatch(appSource, /\bfetch\s*\(/);
  assert.match(worker, /url\.origin === APP_BASE_URL\.origin/);
  assert.match(worker, /url\.pathname\.startsWith\(APP_BASE_URL\.pathname\)/);
  assert.match(worker, /if \(!isWithinScope\(url\)\) return/);
  assert.match(worker, /request\.method !== "GET"/);
});
