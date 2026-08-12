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
const acceptedBrowser = read("tests/browser/webserial-readonly-smoke.mjs");
const productionBrowser = read("tests/webapp-readonly-fc-browser-smoke.mjs");
const sha256 = (value) => crypto.createHash("sha256").update(value).digest("hex");

function numericFixture(source, declaration) {
  const fixture = source.match(
    new RegExp(`const ${declaration} = (\\[[\\s\\S]*?\\])\\.map`),
  );
  assert.ok(fixture?.[1], `missing numeric fixture: ${declaration}`);
  return [...fixture[1].matchAll(/\[[\d,\s]+\]/g)].map((entry) =>
    JSON.parse(entry[0]),
  );
}

test("the bounded production host and declaration are byte-locked", () => {
  assert.equal(sha256(host), "b45009fac582e7c33f761c71ed58201c0fe2cf4b3d7587d6aae9aad1227b3309");
  assert.equal(
    sha256(hostTypes),
    "03d6442a8a9b862e93857029c38a28c78c5cb56d2b8631dd12504a03bbfb9a01",
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

test("production App reuses the exact accepted read-only response fixtures", () => {
  assert.deepEqual(
    numericFixture(productionBrowser, "inScopeReplies"),
    numericFixture(acceptedBrowser, "IN_SCOPE_REPLIES"),
  );
});

test("UI output and state are privacy bounded and make no hardware claim", () => {
  const allowedFields = [
    "apiVersion",
    "fcVariant",
    "fcVersion",
    "targetName",
    "scopeMismatchField",
    "failure",
    "failureStage",
    "failureReason",
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
  assert.match(host, /failureStage: discovery\.failureStage \?\? undefined/);
  assert.match(host, /failureReason: discovery\.failureReason \?\? undefined/);
  assert.match(facadeTypes, /"API_VERSION"[\s\S]*"FC_VARIANT"[\s\S]*"FC_VERSION"[\s\S]*"BOARD_INFO"/);
  assert.match(
    facadeTypes,
    /"WrongCommand"[\s\S]*"WrongDirection"[\s\S]*"ErrorReply"[\s\S]*"ReplyMisclassified"[\s\S]*"WrongLength"[\s\S]*"FieldOverrun"[\s\S]*"TrailingPayload"[\s\S]*"InvalidUtf8"/,
  );
  assert.doesNotMatch(`${facade}\n${facadeTypes}`, /expectedBytes|foundBytes|rawPayload|signature|uid|serialNumber/i);
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
