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

test("product surface has no deferred authority outside the audited storage adapter", () => {
  const files = [path.join(webRoot, "index.html"), ...sourceFiles(path.join(webRoot, "src")), ...sourceFiles(path.join(webRoot, "public"))]
    .filter((file) => !file.endsWith("favicon.svg"));
  const source = files.map((file) => fs.readFileSync(file, "utf8")).join("\n");
  const forbidden = [
    /navigator\s*\.\s*(serial|usb|hid|bluetooth)/i,
    /\b(SerialPort|USBDevice|HIDDevice|BluetoothDevice)\b/,
    /\b(localStorage|sessionStorage)\b/,
    /\b(WebSocket|EventSource|sendBeacon|XMLHttpRequest)\b/,
    /\b(analytics|telemetry)\b/i,
    /\.wasm\b|WebAssembly/,
    /\b(WriteApproval|motor command|buildMspFrame|flashFirmware|dfuUtil)\b/i,
  ];
  for (const pattern of forbidden) assert.doesNotMatch(source, pattern);

  const adapter = path.join(webRoot, "src", "storage", "indexeddb-journal-store.ts");
  const outsideAdapter = files
    .filter((file) => file !== adapter)
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  assert.doesNotMatch(outsideAdapter, /\bindexedDB\b/);
  assert.match(fs.readFileSync(adapter, "utf8"), /globalThis\.indexedDB/);
});

test("the only network primitive is the guarded same-origin service worker fetch", () => {
  const appSource = sourceFiles(path.join(webRoot, "src"))
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  const worker = fs.readFileSync(path.join(webRoot, "public", "sw.js"), "utf8");
  assert.doesNotMatch(appSource, /\bfetch\s*\(/);
  assert.match(worker, /url\.origin !== self\.location\.origin/);
  assert.match(worker, /request\.method !== "GET"/);
});
