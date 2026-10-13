/**
 * The journeys' test fixtures: a private sandbox per test, and `launch`, which starts the app in it
 * and always shuts it down afterwards. When a test fails, every open window's screenshot and the
 * app's output are attached to the report, and the sandbox is kept for inspection.
 */

import { test as base } from "@playwright/test";
import { UnfoldApp } from "./app";
import { Sandbox } from "./sandbox";

export interface Fixtures {
  readonly sandbox: Sandbox;
  readonly launch: () => Promise<UnfoldApp>;
}

/** How long a quit may take before the process is killed (a hung quit fails the test anyway). */
const QUIT_PATIENCE_MS = 20_000;

async function shutDown(app: UnfoldApp): Promise<void> {
  if (app.isClosed) {
    return;
  }
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<"timeout">((resolve) => {
    timer = setTimeout(() => {
      resolve("timeout");
    }, QUIT_PATIENCE_MS);
  });
  const outcome = await Promise.race([app.quit().then(() => "quit" as const), timeout]);
  clearTimeout(timer);
  if (outcome === "timeout") {
    app.electron.process().kill("SIGKILL");
  }
}

export const test = base.extend<Fixtures>({
  // Playwright reads a fixture's dependencies from its first parameter's destructuring.
  // eslint-disable-next-line no-empty-pattern
  sandbox: async ({}, use, testInfo) => {
    const sandbox = Sandbox.create();
    await use(sandbox);
    if (testInfo.status === testInfo.expectedStatus) {
      sandbox.dispose();
    } else {
      testInfo.annotations.push({ type: "sandbox", description: sandbox.root });
    }
  },

  launch: async ({ sandbox }, use, testInfo) => {
    const apps: UnfoldApp[] = [];
    await use(async () => {
      const app = await UnfoldApp.launch(sandbox);
      apps.push(app);
      return app;
    });
    const failed = testInfo.status !== testInfo.expectedStatus;
    for (const [index, app] of apps.entries()) {
      if (failed) {
        if (!app.isClosed) {
          for (const [windowIndex, page] of app.windows().entries()) {
            const path = testInfo.outputPath(`app${index}-window${windowIndex}.png`);
            if (await page.screenshot({ path }).then(() => true, () => false)) {
              await testInfo.attach(`app${index}-window${windowIndex}.png`, { path, contentType: "image/png" });
            }
          }
        }
        await testInfo.attach(`app${index}.log`, { body: app.logs.join(""), contentType: "text/plain" });
      }
      await shutDown(app);
    }
  },
});

export { expect } from "@playwright/test";
