import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The UI is served from the app's own `app://unfold/` origin under a strict content security policy:
// no inline scripts or styles, nothing from the network, and no data: URLs.
export default defineConfig({
  base: "./",
  plugins: [react()],
  server: { port: 5173, strictPort: true },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "chrome140",
    // Fonts and images ship as files; inlining them would need data: in the policy.
    assetsInlineLimit: 0,
    // Chromium supports modulepreload natively; the polyfill would be extra code.
    modulePreload: { polyfill: false },
    sourcemap: true,
    // The window's UI, and the hidden render window that draws pages for images and checks.
    rollupOptions: {
      input: {
        index: fileURLToPath(new URL("index.html", import.meta.url)),
        render: fileURLToPath(new URL("render.html", import.meta.url)),
      },
    },
  },
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    // Component tests opt into a DOM with a `// @vitest-environment jsdom` comment.
    environment: "node",
    setupFiles: ["src/test/setup.ts"],
    // Tests that read a stylesheet as text (`?raw`, the theme's tests) get it; CSS a component imports stays stubbed out.
    css: { include: [/\.css\?raw$/] },
  },
});
