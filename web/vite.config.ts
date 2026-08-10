import { defineConfig } from "vite";

export default defineConfig({
  base: "/",
  build: {
    emptyOutDir: true,
    outDir: "dist",
    rolldownOptions: {
      // This generated ESM module is a byte-locked public asset loaded from the
      // same origin. Keep the root-absolute import for both dev and production.
      external: ["/wasm/ade_web_readonly_serial_wasm_bridge.js"],
    },
  },
  server: {
    host: "127.0.0.1",
  },
});
