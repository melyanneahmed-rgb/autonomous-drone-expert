import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../..", import.meta.url));

function workflow(name) {
  return fs.readFileSync(path.join(repositoryRoot, ".github", "workflows", name), "utf8");
}

function assertManualOnly(source) {
  const triggerBlock = source.match(/^on:\n(?<body>(?: {2}.*\n)+)/m)?.groups?.body ?? "";
  assert.match(triggerBlock, /^  workflow_dispatch:/m);
  assert.doesNotMatch(triggerBlock, /^  (?:push|pull_request|schedule):/m);
  assert.doesNotMatch(source, /pull_request_target/);
}

function assertPinnedActions(source) {
  const uses = [...source.matchAll(/^\s*uses:\s*([^\s#]+)(?:\s*#.*)?$/gm)].map((match) => match[1]);
  assert.ok(uses.length > 0);
  for (const reference of uses) {
    assert.match(reference, /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+@[0-9a-f]{40}$/);
  }
}

test("Web Preview remains manual, fail-closed, immutable, and minimally privileged", () => {
  const source = workflow("web-preview.yml");
  assertManualOnly(source);
  assertPinnedActions(source);
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
  assertPinnedActions(source);
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
