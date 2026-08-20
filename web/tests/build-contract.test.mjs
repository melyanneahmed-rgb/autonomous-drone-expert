import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const provenance = JSON.parse(
  fs.readFileSync(path.join(webRoot, "..", "policy", "webserial-wasm-assets.json"), "utf8"),
);
const sha256 = (file) => crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
const expectedBuildSha = "1111111111111111111111111111111111111111";

function filesBelow(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(directory, entry.name);
    return entry.isDirectory() ? filesBelow(full) : [full];
  });
}

function buildAndInspect(basePath) {
  const vite = path.join(webRoot, "node_modules", "vite", "bin", "vite.js");
  const dist = fs.mkdtempSync(path.join(os.tmpdir(), "ade-web-build-contract-"));
  try {
    const build = spawnSync(process.execPath, [vite, "build", "--outDir", dist], {
      cwd: webRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        ADE_WEB_BASE_PATH: basePath,
        ADE_BUILD_SHA: expectedBuildSha,
      },
      shell: false,
    });
    assert.equal(build.status, 0, `${build.stdout}\n${build.stderr}`);

    const files = filesBelow(dist);
    const relative = files.map((file) => path.relative(dist, file).replaceAll("\\", "/"));
    for (const required of ["index.html", "manifest.webmanifest", "sw.js", "favicon.svg"]) {
      assert.ok(relative.includes(required), `missing dist/${required}`);
    }
    assert.equal(relative.filter((file) => file.endsWith(".html")).length, 1);
    assert.ok(relative.some((file) => file.endsWith(".js")), "built JavaScript missing");
    assert.ok(relative.some((file) => file.endsWith(".css")), "built CSS missing");

    const html = fs.readFileSync(path.join(dist, "index.html"), "utf8");
    assert.doesNotMatch(html, /(?:src|href)=["']https?:\/\//i);
    assert.ok(html.includes(`${basePath}manifest.webmanifest`));
    assert.ok(html.includes(`${basePath}favicon.svg`));
    assert.match(html, new RegExp(`(?:src|href)=["']${basePath.replaceAll("/", "\\/")}assets/`));
    if (basePath !== "/") {
      assert.doesNotMatch(html, /(?:src|href)=["']\/(?!autonomous-drone-expert\/)/);
    }

    const manifest = JSON.parse(fs.readFileSync(path.join(dist, "manifest.webmanifest"), "utf8"));
    assert.equal(manifest.start_url, "./");
    assert.equal(manifest.scope, "./");
    const worker = fs.readFileSync(path.join(dist, "sw.js"), "utf8");
    assert.ok(worker.includes(`const EMBEDDED_BUILD_VERSION = "${expectedBuildSha}";`));
    assert.doesNotMatch(worker, /__ADE_SERVICE_WORKER_BUILD_SHA__/);

    const wasmFiles = relative.filter((file) => file.startsWith("wasm/"));
    assert.deepEqual(wasmFiles.sort(), [
      "wasm/ade_web_readonly_serial_wasm_bridge.js",
      "wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
    ]);
    for (const output of provenance.outputs) {
      const relativeOutput = output.path.replace("web/public/", "");
      assert.equal(sha256(path.join(dist, relativeOutput)), output.sha256, relativeOutput);
    }

    const bundledJavaScript = files
      .filter(
        (file) =>
          file.endsWith(".js") &&
          path.relative(dist, file).replaceAll("\\", "/") !==
            "wasm/ade_web_readonly_serial_wasm_bridge.js",
      )
      .map((file) => fs.readFileSync(file, "utf8"))
      .join("\n");
    const escapedBase = basePath.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    assert.match(
      bundledJavaScript,
      new RegExp(`from["']${escapedBase}wasm/ade_web_readonly_serial_wasm_bridge\\.js["']`),
      "production bundle must retain the base-scoped generated-module import",
    );
    if (basePath !== "/") {
      assert.doesNotMatch(
        bundledJavaScript,
        /from["']\/wasm\/ade_web_readonly_serial_wasm_bridge\.js["']/,
      );
    }

    const text = files
      .filter((file) => !/\.(?:svg|ico|png|jpg|jpeg|gif)$/i.test(file))
      .map((file) => fs.readFileSync(file, "utf8"))
      .join("\n");
    assert.doesNotMatch(text, /(?:vinext|cloudflare|wrangler|sites-vite-plugin|next\/)/i);
    assert.ok(text.includes(basePath));
  } finally {
    fs.rmSync(dist, { recursive: true, force: true });
  }
}

test("production builds preserve the approved PWA at root and repository scope", () => {
  buildAndInspect("/");
  buildAndInspect("/autonomous-drone-expert/");
});
