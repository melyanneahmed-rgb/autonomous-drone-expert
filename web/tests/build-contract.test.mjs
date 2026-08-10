import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const dist = path.join(webRoot, "dist");
const provenance = JSON.parse(
  fs.readFileSync(path.join(webRoot, "..", "policy", "webserial-wasm-assets.json"), "utf8"),
);

const sha256 = (file) => crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");

function filesBelow(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(directory, entry.name);
    return entry.isDirectory() ? filesBelow(full) : [full];
  });
}

test("production build contains only the approved static PWA surface", () => {
  const vite = path.join(
    webRoot,
    "node_modules",
    ".bin",
    process.platform === "win32" ? "vite.cmd" : "vite",
  );
  const build = spawnSync(vite, ["build"], {
    cwd: webRoot,
    encoding: "utf8",
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

  const wasmFiles = relative.filter((file) => file.startsWith("wasm/"));
  assert.deepEqual(wasmFiles.sort(), [
    "wasm/ade_web_readonly_serial_wasm_bridge.js",
    "wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm",
  ]);
  for (const output of provenance.outputs) {
    const relativeOutput = output.path.replace("web/public/", "");
    assert.equal(sha256(path.join(dist, relativeOutput)), output.sha256, relativeOutput);
  }

  const html = fs.readFileSync(path.join(dist, "index.html"), "utf8");
  assert.doesNotMatch(html, /(?:src|href)=["']https?:\/\//i);
  const text = files
    .filter((file) => !/\.(?:svg|ico|png|jpg|jpeg|gif)$/i.test(file))
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n");
  assert.doesNotMatch(text, /(?:vinext|cloudflare|wrangler|sites-vite-plugin|next\/)/i);
});
