import { defineConfig } from "vite";

export default defineConfig({
  base: "/",
  build: {
    emptyOutDir: true,
    outDir: "dist",
  },
  server: {
    host: "127.0.0.1",
  },
});
