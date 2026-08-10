import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const read = (relative) => fs.readFileSync(path.join(webRoot, relative), "utf8");
const app = read("src/App.tsx");
const facade = read("src/connection/readonly-fc-connection.mjs");
const facadeTypes = read("src/connection/readonly-fc-connection.d.mts");
const host = read("src/transport/webserial-readonly-host.mjs");
const hostTypes = read("src/transport/webserial-readonly-host.d.mts");
const viteConfig = read("vite.config.ts");
const productionBrowser = read("tests/webapp-readonly-fc-browser-smoke.mjs");
const sha256 = (value) => crypto.createHash("sha256").update(value).digest("hex");

test("the accepted production host remains byte-for-byte unchanged", () => {
  assert.equal(sha256(host), "a27c3f885ccff82041f85a7f6febc38ab80a9bac6a985320d4f49f68f0350973");
  assert.equal(
    sha256(hostTypes),
    "6c16e032e9fbbad7ace75d6a29ecc83ff5d792f8198fa3cf55cb647e0a76ee61",
  );
});

test("React has only the narrow prepared connection facade", () => {
  assert.match(
    app,
    /import \{ prepareReadonlyFcConnection \} from "\.\/connection\/readonly-fc-connection\.mjs"/,
  );
  assert.doesNotMatch(app, /webserial-readonly-host|WasmReadonlySerial|navigator\s*\.\s*serial/i);
  assert.doesNotMatch(app, /\b(requestPort|getPorts|getInfo)\s*\(/);
  assert.doesNotMatch(
    app,
    /\b(?:MSP_[A-Z0-9_]+|CommandId|WriteApproval|TransportEffect|SerialPort)\b|\b(?:build|encode|decode)Msp/i,
  );
  assert.doesNotMatch(app, /writer\s*\.\s*write|sendRaw|writeRaw|commandId|payload/i);
  assert.match(app, /void prepareReadonlyFcConnection\(\)/);
  assert.match(app, /readonlyFcConnection\.current = prepared;\s*setConnection\(\{ phase: "ready" \}\)/s);
  assert.match(app, /disabled=\{!connectionEnabled\}/);
});

test("the click path selects first and discovers with zero arguments", () => {
  const clickFlow = app.match(/async function connectReadonlyFc\(\) \{(?<body>.*?)^  \}/ms);
  assert.ok(clickFlow?.groups?.body);
  const body = clickFlow.groups.body;
  const selection = body.indexOf("await prepared.selectPortFromUserGesture()");
  const discovery = body.indexOf("await prepared.discover()");
  assert.ok(selection >= 0 && discovery > selection);
  assert.doesNotMatch(body.slice(0, selection), /\bawait\b/);
  assert.match(facade, /const selection = await this\.#host\.selectPortFromUserGesture\(\)/);
  assert.match(facade, /await this\.#host\.discover\(\)/);
  assert.doesNotMatch(facade, /\.discover\s*\(\s*[^)]/);
  assert.match(facadeTypes, /discover\(\): Promise<PrivacyBoundedIdentityResult>/);
});

test("facade exports no command, payload, raw transport, or replaceable host authority", () => {
  assert.deepEqual(
    [...facade.matchAll(/^import .* from "([^"]+)";$/gm)].map((match) => match[1]),
    [
      "/wasm/ade_web_readonly_serial_wasm_bridge.js",
      "../transport/webserial-readonly-host.mjs",
    ],
  );
  assert.doesNotMatch(
    `${facade}\n${facadeTypes}`,
    /\b(?:MSP_[A-Z0-9_]+|CommandId|WriteApproval|TransportEffect|sendRaw|writeRaw|commandId|payload)\b/i,
  );
  assert.doesNotMatch(facadeTypes, /constructor|serial\?:|host\?:|bindings|factory/i);
  assert.doesNotMatch(facade, /navigator\s*\.\s*serial|requestPort|getPorts|getInfo/i);
  assert.match(facade, /class PreparedReadonlyFcConnection/);
  assert.doesNotMatch(facade, /export\s+(?:class|\{[^}]*PreparedReadonlyFcConnection)/);
});

test("Vite externalizes only the byte-locked same-origin generated module", () => {
  const external = viteConfig.match(/external:\s*\[(?<entries>[^\]]*)\]/s);
  assert.ok(external?.groups?.entries, "missing bounded Rolldown external list");
  assert.deepEqual(
    [...external.groups.entries.matchAll(/["']([^"']+)["']/g)].map((match) => match[1]),
    ["/wasm/ade_web_readonly_serial_wasm_bridge.js"],
  );
  assert.match(viteConfig, /rolldownOptions:\s*\{/);
  assert.doesNotMatch(external.groups.entries, /\*|RegExp|new\s+URL|https?:/i);
});

test("production browser gate uses a trusted native gesture rather than DOM click injection", () => {
  assert.match(productionBrowser, /Input\.dispatchKeyEvent/);
  assert.match(productionBrowser, /navigator\.userActivation\?\.isActive === true/);
  assert.doesNotMatch(productionBrowser, /\.click\s*\(/);
});

test("UI output and state are privacy bounded and make no hardware claim", () => {
  const allowedFields = [
    "apiVersion",
    "fcVariant",
    "fcVersion",
    "targetName",
    "scopeMismatchField",
    "failure",
  ];
  assert.deepEqual(
    [...app.matchAll(/data-identity-field="([A-Za-z]+)"/g)].map((match) => match[1]),
    allowedFields,
  );
  assert.deepEqual(
    [...facade.matchAll(/^    ([A-Za-z]+): result\./gm)].map((match) => match[1]),
    ["outcome", ...allowedFields],
  );
  const source = `${app}\n${facade}`;
  assert.doesNotMatch(
    source,
    /serial.?number|\bCOM\d+\b|usbVendorId|usbProductId|\bVID\b|\bPID\b|getInfo|deviceId|unique.?id|raw.?frame/i,
  );
  assert.doesNotMatch(source, /localStorage|sessionStorage|indexedDB|document\.cookie/);
  assert.match(facade, /result\.hardwareObserved !== false/);
  assert.doesNotMatch(app, /hardwareObserved/);
  assert.doesNotMatch(app, /\b(?:CONNECTED|SUPPORTED|VALIDATED)\b/);
  for (const phase of [
    "preparing",
    "ready",
    "selecting",
    "reading-identity",
    "read-complete",
    "scope-mismatch",
    "cancelled",
    "unavailable",
    "failed",
  ]) {
    assert.ok(app.includes(`"${phase}"`), `missing bounded UI phase: ${phase}`);
  }
});
