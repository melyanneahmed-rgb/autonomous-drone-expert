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
const diagnostic = read("src/diagnostics/readonly-trace.mjs");
const diagnosticTypes = read("src/diagnostics/readonly-trace.d.mts");
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
  assert.equal(sha256(host), "9a8bb955292bad61a20062e41b92c8ca1cdb66b3d7d2d9110fbc8cad179385f9");
  assert.equal(
    sha256(hostTypes),
    "fed3d8e35d89a95b4efd20a53044d83748421fe5c23927d21b83d576c3d3ac0a",
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
    "failureOrigin",
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

test("diagnostic UI is collapsed, RAM-only, owner-copyable, and owner-clearable", () => {
  assert.match(app, /<details className="diagnostic-trace" data-diagnostic-trace="temporary">/);
  assert.doesNotMatch(app, /<details[^>]*\bopen(?:=|\s|>)/);
  assert.match(app, /prepared\.safeDiagnosticTraceText\(\)/);
  assert.match(app, /readonlyFcConnection\.current\?\.clearDiagnosticTrace\(\)/);
  assert.match(host, /discovery\.takeTraceEvent\(\)/);
  assert.match(diagnostic, /DIAGNOSTIC_TRACE_CAPACITY = 200/);
  assert.match(diagnostic, /capacity < 100 \|\| capacity > 250/);
  assert.match(diagnostic, /this\.#events\.splice\(0, this\.#events\.length - this\.#capacity\)/);
  assert.doesNotMatch(
    `${diagnostic}\n${host}`,
    /console\.|localStorage|sessionStorage|indexedDB|document\.cookie|fetch\(|XMLHttpRequest|WebSocket|sendBeacon/,
  );
  assert.doesNotMatch(
    `${diagnostic}\n${diagnosticTypes}`,
    /rawBytes|rawFrame|rawPayload|usbVendorId|usbProductId|serialNumber|portInfo/i,
  );
  for (const layer of ["UI", "HOST", "RUST", "SERIAL", "MSP", "CLEANUP"]) {
    assert.ok(diagnostic.includes(`"${layer}"`), `missing diagnostic layer: ${layer}`);
  }
  for (const origin of [
    "PORT_SELECTION",
    "PORT_OPEN",
    "WRITER_ACQUISITION",
    "READER_ACQUISITION",
    "SERIAL_WRITE",
    "SERIAL_READ",
    "SERIAL_TIMEOUT",
    "MSP_FRAME",
    "IDENTITY_STAGE",
    "DIRECTIVE_REFUSAL",
    "PORT_CLOSE",
    "UI_BOUNDARY",
  ]) {
    assert.ok(diagnostic.includes(`"${origin}"`), `missing fixed diagnostic origin: ${origin}`);
  }
});
