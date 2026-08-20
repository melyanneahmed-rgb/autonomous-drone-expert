import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const read = (relative) => fs.readFileSync(new URL(`../${relative}`, import.meta.url), "utf8");

test("manifest defines the approved standalone Arabic shell", () => {
  const manifest = JSON.parse(read("public/manifest.webmanifest"));
  assert.equal(manifest.name, "Smart Configurator");
  assert.equal(manifest.lang, "ar");
  assert.equal(manifest.dir, "rtl");
  assert.equal(manifest.start_url, "./");
  assert.equal(manifest.scope, "./");
  assert.equal(manifest.display, "standalone");
  assert.ok(manifest.icons.some((icon) => icon.src === "favicon.svg"));
});

test("service worker is base-scoped, commit-versioned, GET-only, and update safe", () => {
  const register = read("src/pwa-register.ts");
  const worker = read("public/sw.js");
  assert.match(register, /import\.meta\.env\.BASE_URL/);
  assert.match(register, /new URL\("sw\.js", baseUrl\)/);
  assert.match(register, /searchParams\.set\("version", __ADE_BUILD_SHA__\)/);
  assert.match(register, /scope: baseUrl\.pathname/);
  assert.match(register, /updateViaCache: "none"/);
  assert.match(register, /getRegistration\(baseUrl\.href\)/);
  assert.match(register, /existingRegistration\.unregister\(\)/);
  assert.match(register, /new MessageChannel\(\)/);
  assert.match(register, /CACHE_RESOURCES_COMPLETE/);
  assert.match(register, /window\.location\.origin/);
  for (const asset of [
    "wasm/ade_web_storage_wasm_bridge.js",
    "wasm/ade_web_storage_wasm_bridge_bg.wasm",
    "wasm/ade_web_readonly_serial_wasm_bridge.js",
    "wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
  ]) {
    assert.ok(register.includes(`"${asset}"`), `missing offline runtime asset: ${asset}`);
  }
  assert.match(worker, /request\.method !== "GET"/);
  assert.match(worker, /new URL\("\.\/", self\.registration\.scope\)/);
  assert.match(worker, /EMBEDDED_BUILD_VERSION = "__ADE_SERVICE_WORKER_BUILD_SHA__"/);
  assert.match(worker, /requestedVersion !== BUILD_VERSION/);
  assert.match(worker, /CACHE_NAME = `\$\{CACHE_PREFIX\}\$\{BUILD_VERSION\}`/);
  assert.match(worker, /relativePath\.startsWith\("wasm\/"\)/);
  assert.match(worker, /networkFirst\(request, APP_BASE_URL\.href\)/);
  assert.match(worker, /event\.ports\[0\]\?\.postMessage/);
  for (const asset of ["manifest.webmanifest", "favicon.svg"]) {
    assert.ok(worker.includes(`"${asset}"`), `missing app-shell asset: ${asset}`);
  }
  assert.doesNotMatch(worker, /["']\/(?:manifest\.webmanifest|favicon\.svg|wasm\/)/);
  assert.doesNotMatch(worker, /https?:\/\//i);
  assert.doesNotMatch(worker, /\b(indexedDB|ADEJ|journal|casebook)\b/i);
  assert.doesNotMatch(worker, /serialNumber|getInfo|usbVendorId|usbProductId|deviceId/i);
});
