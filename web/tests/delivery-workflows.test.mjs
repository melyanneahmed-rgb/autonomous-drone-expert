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
  assert.doesNotMatch(source, /secrets\./);
  assert.doesNotMatch(source, /\bgit\s+(?:push|merge|rebase|reset|clean)\b/);
});
