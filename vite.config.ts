import { defineConfig } from "vite";

// Tauri expects a fixed dev port and a relative build.
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true },
});
