import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
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
        ADE_BUILD_SHA: "1111111111111111111111111111111111111111",
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
