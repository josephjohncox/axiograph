import { defineConfig } from "vite";

export default defineConfig(({ mode }) => ({
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: mode === "debug",
    minify: mode === "debug" ? false : "esbuild"
  }
}));
