import fs from "node:fs";
import path from "node:path";
import { defineConfig } from "vite";

const READONLY_WASM_MODULE_ID = "virtual:ade-web-readonly-serial-wasm";
const SERVICE_WORKER_BUILD_TOKEN = "__ADE_SERVICE_WORKER_BUILD_SHA__";

function normalizedBasePath(value: string | undefined): string {
  const candidate = value?.trim() || "/";
  if (!candidate.startsWith("/") || /[?#]/.test(candidate)) {
    throw new Error("ADE_WEB_BASE_PATH must be an absolute URL path without query or fragment");
  }
  const withoutTrailingSlash = candidate.replace(/\/+$/, "");
  return withoutTrailingSlash ? `${withoutTrailingSlash}/` : "/";
}

const base = normalizedBasePath(process.env.ADE_WEB_BASE_PATH);
const buildSha = process.env.ADE_BUILD_SHA?.trim() || "local-development";

if (!/^(?:[0-9a-f]{7,64}|local-development)$/.test(buildSha)) {
  throw new Error("ADE_BUILD_SHA must be a lowercase hexadecimal commit SHA");
}

export default defineConfig({
  base,
  define: {
    __ADE_BUILD_SHA__: JSON.stringify(buildSha),
  },
  plugins: [
    {
      name: "ade-service-worker-build-stamp",
      apply: "build",
      writeBundle(options) {
        if (!options.dir) throw new Error("Vite build output directory is required");
        const workerPath = path.resolve(options.dir, "sw.js");
        const source = fs.readFileSync(workerPath, "utf8");
        const tokenCount = source.split(SERVICE_WORKER_BUILD_TOKEN).length - 1;
        if (tokenCount !== 1) {
          throw new Error(`Expected one service-worker build token, found ${tokenCount}`);
        }
        fs.writeFileSync(workerPath, source.replace(SERVICE_WORKER_BUILD_TOKEN, buildSha));
      },
    },
    {
      name: "ade-web-readonly-serial-wasm-base-path",
      resolveId(source) {
        if (source !== READONLY_WASM_MODULE_ID) return null;
        return {
          id: `${base}wasm/ade_web_readonly_serial_wasm_bridge.js`,
          external: true,
        };
      },
    },
  ],
  build: {
    emptyOutDir: true,
    outDir: "dist",
  },
  server: {
    host: "127.0.0.1",
  },
});
