import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../..", import.meta.url));
const actionPolicy = JSON.parse(
  fs.readFileSync(
    path.join(repositoryRoot, "policy", "github-actions-allowlist.json"),
    "utf8",
  ),
);
const deliveryActionAllowlist = new Set(actionPolicy.delivery_actions);

function workflow(name) {
  return fs.readFileSync(path.join(repositoryRoot, ".github", "workflows", name), "utf8");
}

function actionReferences(source) {
  return [...source.matchAll(/^\s*uses:\s*([^\s#]+)(?:\s*#.*)?$/gm)].map(
    (match) => match[1],
  );
}

function assertManualOnly(source) {
  const triggerBlock = source.match(/^on:\n(?<body>(?: {2}.*\n)+)/m)?.groups?.body ?? "";
  assert.match(triggerBlock, /^  workflow_dispatch:/m);
  assert.doesNotMatch(triggerBlock, /^  (?:push|pull_request|schedule):/m);
  assert.doesNotMatch(source, /pull_request_target/);
}

function assertImmutableDeliveryActions(source) {
  const references = actionReferences(source);
  assert.ok(references.length > 0);
  for (const reference of references) {
    assert.match(reference, /^actions\/[A-Za-z0-9_.-]+@[0-9a-f]{40}$/);
    assert.ok(
      deliveryActionAllowlist.has(reference),
      `unexpected delivery action reference: ${reference}`,
    );
    assert.ok(
      !actionPolicy.canonical_ci_temporary_exceptions.includes(reference),
      `canonical CI exception spread into delivery: ${reference}`,
    );
  }
}

function assertAppearsInOrder(source, needles) {
  let cursor = -1;
  for (const needle of needles) {
    const next = source.indexOf(needle, cursor + 1);
    assert.ok(next > cursor, `expected ordered workflow fragment: ${needle}`);
    cursor = next;
  }
}

function assertCanonicalSerialWasmDelivery(source, { browserGateCount }) {
  assert.match(source, /rustup toolchain install 1\.85\.0 --profile minimal --target wasm32-unknown-unknown/);
  assert.match(source, /rustup toolchain install 1\.97\.1 --profile minimal/);
  assert.match(source, /cargo \+1\.85\.0 build --locked --release --target wasm32-unknown-unknown -p ade-web-readonly-serial-wasm-bridge/);
  assert.match(source, /--remap-path-prefix=\/home\/runner\/\.cargo\/registry\/src\/index\.crates\.io-1949cf8c6b5b557f\/wasm-bindgen-0\.2\.127\/src\/convert\/slices\.rs=\/source\/wasm-bindgen\/src\/convert\/slices\.rs/);
  assert.match(source, /--remap-path-prefix=\/home\/runner\/\.cargo\/registry\/src\/index\.crates\.io-1949cf8c6b5b557f\/wasm-bindgen-0\.2\.127\/src\/externref\.rs=\/source\/wasm-bindgen\/src\/externref\.rs/);
  assert.match(source, /--remap-path-prefix=crates\/protocol-msp\/src\/lib\.rs=\/source\/project\/crates\/protocol-msp\/src\/lib\.rs/);
  assert.match(source, /cargo \+1\.97\.1 run --locked --manifest-path tools\/wasm-bindgen-cli-support\/Cargo\.toml/);
  assert.match(source, /scripts\/verify_webserial_product_assets\.py/);
  assert.match(source, /--input-wasm target\/wasm32-unknown-unknown\/release\/ade_web_readonly_serial_wasm_bridge\.wasm/);
  assert.match(source, /--generated-dir target\/webserial-wasm-product-regenerated/);
  assert.match(source, /scripts\/prepare_web_wasm\.py/);
  assert.match(source, /--serial-root target\/webserial-wasm-product-regenerated/);
  assert.equal(
    (source.match(/webserial-readonly-browser-smoke\.mjs \.\.\/target\/webserial-wasm-product-regenerated/g) ?? []).length,
    browserGateCount,
  );
  assert.doesNotMatch(source, /target\/webserial-wasm-web/);
  assert.match(source, /git diff --exit-code --[\s\S]*web\/public\/wasm\/ade_web_readonly_serial_wasm_bridge\.js[\s\S]*web\/public\/wasm\/ade_web_readonly_serial_wasm_bridge_bg\.wasm/);
  assert.match(source, /cmp --silent[\s\S]*web\/public\/wasm\/ade_web_readonly_serial_wasm_bridge\.js[\s\S]*web\/dist\/wasm\/ade_web_readonly_serial_wasm_bridge\.js/);
  assert.match(source, /cmp --silent[\s\S]*web\/public\/wasm\/ade_web_readonly_serial_wasm_bridge_bg\.wasm[\s\S]*web\/dist\/wasm\/ade_web_readonly_serial_wasm_bridge_bg\.wasm/);
}

test("delivery workflows use exactly the immutable selected-action allowlist", () => {
  const references = new Set([
    ...actionReferences(workflow("web-preview.yml")),
    ...actionReferences(workflow("android-apk.yml")),
  ]);
  assert.deepEqual([...references].sort(), [...deliveryActionAllowlist].sort());
});

test("temporary canonical CI tag exception remains confined to canonical CI", () => {
  assert.deepEqual(actionPolicy.canonical_ci_temporary_exceptions, [
    "actions/checkout@v7.0.1",
  ]);

  const ciReferences = new Set(actionReferences(workflow("ci.yml")));
  assert.deepEqual(
    [...ciReferences].sort(),
    [...actionPolicy.canonical_ci_temporary_exceptions].sort(),
  );

  for (const name of ["web-preview.yml", "android-apk.yml"]) {
    const references = actionReferences(workflow(name));
    for (const exception of actionPolicy.canonical_ci_temporary_exceptions) {
      assert.ok(
        !references.includes(exception),
        `canonical CI tag exception appeared in ${name}: ${exception}`,
      );
    }
  }
});

test("Web Preview remains manual, fail-closed, immutable, and minimally privileged", () => {
  const source = workflow("web-preview.yml");
  assertManualOnly(source);
  assertImmutableDeliveryActions(source);
  assert.match(source, /^permissions:\n  actions: read\n  contents: read$/m);
  assert.match(source, /^    permissions:\n      pages: write\n      id-token: write$/m);
  assert.equal((source.match(/pages: write/g) ?? []).length, 1);
  assert.match(source, /scripts\/require_successful_ci\.py/);
  assert.match(source, /scripts\/hash_directory\.py web\/dist/);
  assert.match(source, /scripts\/check_public_web_artifact\.py web\/dist/);
  assert.match(source, /actions\/upload-pages-artifact@/);
  assert.match(source, /actions\/deploy-pages@/);
  assert.match(source, /\/autonomous-drone-expert\//);
  assert.match(source, /production-delivery-browser-smoke\.mjs/);
  assertCanonicalSerialWasmDelivery(source, { browserGateCount: 2 });
  assertAppearsInOrder(source, [
    "cargo +1.97.1 run --locked --manifest-path tools/wasm-bindgen-cli-support/Cargo.toml",
    "python3 scripts/verify_webserial_product_assets.py",
    "python3 scripts/prepare_web_wasm.py",
    "git diff --exit-code --",
    "Exercise existing browser storage and Web Serial gates at both scopes",
    "Exercise repository-subpath production build, offline shell, and update transition",
    "Prove final Pages dist contains canonical serial product assets",
    "Public Pages artifact privacy gate",
  ]);
  assert.doesNotMatch(source, /secrets\./);
  assert.doesNotMatch(source, /\bgit\s+(?:push|merge|rebase|reset|clean)\b/);
});

test("Android APK remains manual, read-only, hashed, and validation-only", () => {
  const source = workflow("android-apk.yml");
  assertManualOnly(source);
  assertImmutableDeliveryActions(source);
  assert.match(source, /^permissions:\n  actions: read\n  contents: read$/m);
  assert.doesNotMatch(source, /^\s+(?:pages|packages|deployments|pull-requests|id-token):\s*write$/m);
  assert.match(source, /scripts\/require_successful_ci\.py/);
  assert.match(source, /scripts\/check_android_validation\.py/);
  assert.match(source, /scripts\/check_public_web_artifact\.py web\/dist/);
  assert.match(source, /assembleDebug/);
  assert.match(source, /sha256sum/);
  assert.match(source, /actions\/upload-artifact@/);
  assert.match(source, /DEVELOPMENT \/ VALIDATION — NOT PRODUCTION SIGNED/);
  assert.match(source, /Android USB flight-controller support: NOT VALIDATED/);
  assertCanonicalSerialWasmDelivery(source, { browserGateCount: 1 });
  assertAppearsInOrder(source, [
    "cargo +1.97.1 run --locked --manifest-path tools/wasm-bindgen-cli-support/Cargo.toml",
    "python3 scripts/verify_webserial_product_assets.py",
    "python3 scripts/prepare_web_wasm.py",
    "git diff --exit-code --",
    "Exercise repository-subpath PWA, IndexedDB, Web Serial, and WASM",
    "Prove final Android Web dist contains canonical serial product assets",
    "Prove packaged PWA contains no private material",
    "Validate Android authority and dependency policy",
    "Compile debug validation APK from strict lock and checksums",
  ]);
  assert.doesNotMatch(source, /secrets\./);
  assert.doesNotMatch(source, /\bgit\s+(?:push|merge|rebase|reset|clean)\b/);
});
