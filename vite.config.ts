import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const { version } = JSON.parse(readFileSync(resolve(import.meta.dirname, "package.json"), "utf8")) as { version: string };

// Tauri expects a fixed dev port and must not clear the terminal.
export default defineConfig(({ mode }) => ({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": resolve(import.meta.dirname, "src") } },
  define: { __APP_VERSION__: JSON.stringify(version) },
  clearScreen: false,
  // The dev server must not watch the Rust side: while cargo compiles, Windows locks the files in
  // `target/` and Vite's watcher crashed with EBUSY (seen 5 Nov 2026 on a first `pnpm tauri dev`).
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/target/**", "**/src-tauri/**", "**/crates/**", "**/docs/**"] } },
  build: {
    target: "es2022",
    rollupOptions: {
      // Entry points (TRD §8): the main window, the pet window (F1-22) and the pet's hover card (own window since 0.5.8).
      // `--mode preview` builds one self-contained bundle for design previews.
      input: {
        main: resolve(import.meta.dirname, "main.html"),
        ...(mode === "preview" ? {} : { pet: resolve(import.meta.dirname, "pet.html") }),
      } as Record<string, string>,
      ...(mode === "preview" ? { output: { inlineDynamicImports: true } } : {}),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    // The first test in a file pays for loading the whole app; on the Windows CI runner that
    // alone took over the 5 s default (2026-10-12). Locally the slowest test takes about 2 s.
    testTimeout: 15_000,
    setupFiles: ["src/test-setup.ts"],
    coverage: {
      provider: "v8", include: ["src/**"], exclude: ["src/**/*.test.{ts,tsx}", "src/ipc/generated/**"],
      // The floor (G6). Measured 2026-10-11: lines 88, branches 80, functions 65. Raise it as it grows; never lower it to pass.
      thresholds: { lines: 80, statements: 80, branches: 70, functions: 55 },
    },
  },
}));
