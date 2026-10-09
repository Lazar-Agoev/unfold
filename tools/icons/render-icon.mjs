// Renders desktop/build/icon.svg to desktop/build/icon.png (1024 x 1024, transparent corners) with
// Electron's own Chromium, so no image tools are needed. electron-builder derives the macOS .icns,
// the Windows .ico, and the Linux icon sizes from that PNG.
//
// Run from the repository root: pnpm --filter @unfold/desktop exec electron ../tools/icons/render-icon.mjs

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { app, BrowserWindow } from "electron";

const SIZE = 1024;
const buildDir = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "desktop", "build");
const svg = readFileSync(join(buildDir, "icon.svg"), "utf8");

app.commandLine.appendSwitch("force-device-scale-factor", "1");
app.dock?.hide();

// Electron can't become ready while an ES module's top-level await is pending, so no await here.
void app.whenReady().then(async () => {
  const window = new BrowserWindow({
    width: SIZE,
    height: SIZE,
    useContentSize: true,
    show: false,
    frame: false,
    transparent: true,
    webPreferences: { offscreen: true, sandbox: true, contextIsolation: true },
  });
  const page = `<!doctype html><html><head><style>html,body{margin:0;background:transparent}svg{display:block}</style></head><body>${svg}</body></html>`;
  // An offscreen window hands over each painted frame; keep the first full frame after the page loads.
  const loaded = window.loadURL(`data:text/html;base64,${Buffer.from(page).toString("base64")}`);
  const frame = new Promise((resolve) => {
    window.webContents.on("paint", (_event, _dirty, image) => {
      if (image.getSize().width >= SIZE) {
        resolve(image);
      }
    });
  });
  await loaded;
  const image = await frame;
  const sized = image.getSize().width === SIZE ? image : image.resize({ width: SIZE, height: SIZE, quality: "best" });
  writeFileSync(join(buildDir, "icon.png"), sized.toPNG());
  console.log(`wrote ${join(buildDir, "icon.png")} (${sized.getSize().width}x${sized.getSize().height})`);
  app.quit();
});
