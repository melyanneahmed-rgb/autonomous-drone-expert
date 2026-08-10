import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const read = (relative) => fs.readFileSync(new URL(`../${relative}`, import.meta.url), "utf8");

test("manifest defines the approved standalone Arabic shell", () => {
  const manifest = JSON.parse(read("public/manifest.webmanifest"));
  assert.equal(manifest.name, "Smart Configurator");
  assert.equal(manifest.lang, "ar");
  assert.equal(manifest.dir, "rtl");
  assert.equal(manifest.start_url, "/");
  assert.equal(manifest.scope, "/");
  assert.equal(manifest.display, "standalone");
  assert.ok(manifest.icons.some((icon) => icon.src === "/favicon.svg"));
});

test("service worker is same-origin, GET-only, and caches required assets", () => {
  const register = read("src/pwa-register.ts");
  const worker = read("public/sw.js");
  assert.match(register, /serviceWorker\.register\("\/sw\.js"\)/);
  assert.match(register, /window\.location\.origin/);
  assert.match(worker, /request\.method !== "GET"/);
  assert.match(worker, /url\.origin !== self\.location\.origin/);
  for (const asset of [
    "/",
    "/manifest.webmanifest",
    "/favicon.svg",
    "/wasm/ade_web_readonly_serial_wasm_bridge.js",
    "/wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
  ]) {
    assert.ok(worker.includes(`"${asset}"`), `missing app-shell asset: ${asset}`);
  }
  assert.doesNotMatch(worker, /https?:\/\//i);
  assert.doesNotMatch(worker, /\b(indexedDB|ADEJ|journal|casebook)\b/i);
  assert.doesNotMatch(worker, /serialNumber|getInfo|usbVendorId|usbProductId|deviceId/i);
});
