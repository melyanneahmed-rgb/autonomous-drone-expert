import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = path.dirname(webRoot);
const readWeb = (relative) => fs.readFileSync(path.join(webRoot, relative), "utf8");
const sha256 = (relative) =>
  crypto.createHash("sha256").update(fs.readFileSync(path.join(webRoot, relative))).digest("hex");

test("only the audited adapter owns browser journal persistence", () => {
  const adapter = readWeb("src/storage/indexeddb-journal-store.ts");
  const contract = readWeb("src/storage/journal-storage-contract.mjs");
  assert.match(adapter, /JOURNAL_DATABASE_NAME = "autonomous-drone-expert-journal"/);
  assert.match(adapter, /JOURNAL_OBJECT_STORE_NAME = "journals"/);
  assert.match(adapter, /database\.transaction\(JOURNAL_OBJECT_STORE_NAME, "readwrite"\)/);
  assert.match(adapter, /isExpectedJournalObjectStoreSchema/);
  assert.match(adapter, /semanticFailure = validated\.failure/);
  assert.match(adapter, /transaction\.addEventListener\("complete"/);
  assert.doesNotMatch(adapter, /putRequest\.addEventListener\("success"/);
  assert.doesNotMatch(`${adapter}\n${contract}`, /\b(console|localStorage|sessionStorage|cookie)\b|deleteDatabase|storage\.persist/);
});
test("storage source has no network, hardware, identity, firmware, or telemetry authority", () => {
  const source = [
    "src/storage/indexeddb-journal-store.ts",
    "src/storage/journal-storage-contract.mjs",
  ].map(readWeb).join("\n");
  for (const pattern of [
    /\b(fetch|WebSocket|EventSource|XMLHttpRequest|sendBeacon)\b/,
    /navigator\s*\.\s*(serial|usb|hid|bluetooth)/i,
    /\b(SerialPort|USBDevice|HIDDevice|BluetoothDevice|requestPort|requestDevice)\b/,
    /\b(serial number|board uid|gps|home coordinates|firmware file|analytics|telemetry)\b/i,
    /\b(WriteApproval|motor command|flashFirmware|dfuUtil)\b/i,
  ]) {
    assert.doesNotMatch(source, pattern);
  }
});

test("React, bootstrap, and service worker do not own or invoke the journal adapter", () => {
  for (const relative of ["src/App.tsx", "src/main.tsx", "public/sw.js"]) {
    const source = readWeb(relative);
    assert.doesNotMatch(source, /indexeddb-journal-store|journal-storage-contract|\bindexedDB\b/);
  }
});

test("approved visible UI and dependency lock remain byte-for-byte frozen", () => {
  assert.equal(sha256("src/App.tsx"), "1d4d44c43832d9fd17d4e0f594114814426e44f6017b14d550e04e990b8c98f9");
  assert.equal(sha256("src/styles.css"), "d74f07088a3b206fc66661eea4682f083f6b7a1b08dbaa5b399818672356a3c4");
  assert.equal(sha256("package.json"), "59c01b1db0bb904a1be453371cfaf8c27a883ebceb0b55bb07e65bc22388a586");
  assert.equal(sha256("package-lock.json"), "c3015e9454da094d307975921b8aa2c195a15b9dffe0498a9c758b57d922c05d");
  const policy = JSON.parse(fs.readFileSync(path.join(repoRoot, "policy", "web-dependencies.json"), "utf8"));
  assert.equal(policy.approved_lockfile_sha256, "c3015e9454da094d307975921b8aa2c195a15b9dffe0498a9c758b57d922c05d");
});
