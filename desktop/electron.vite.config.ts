import { resolve } from "node:path";
import { defineConfig } from "electron-vite";

// Three targets: the main process, the engine host (a utility process, built beside main), and the
// sandboxed preload script. The renderer is the separate `ui` package, built by its own Vite config.
export default defineConfig({
  main: {
    build: {
      outDir: "out/main",
      // electron-updater is bundled into the main process, so the packaged app ships no node_modules.
      externalizeDeps: { exclude: ["electron-updater"] },
      rollupOptions: {
        input: {
          index: resolve(import.meta.dirname, "src/main/index.ts"),
          "engine-host": resolve(import.meta.dirname, "src/engine-host/index.ts"),
        },
      },
    },
  },
  preload: {
    build: {
      outDir: "out/preload",
      rollupOptions: {
        input: { index: resolve(import.meta.dirname, "src/preload/index.ts") },
      },
    },
  },
});
