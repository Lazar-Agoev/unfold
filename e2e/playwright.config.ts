import { defineConfig } from "@playwright/test";

/**
 * The journeys launch the built, unpackaged app (`ui/dist`, `desktop/out`, and the addon in
 * `crates/unfold-node`); `just e2e` builds them first. Each test gets its own user-data folder,
 * home, and copies of the playgrounds, so tests never share state.
 *
 * Two projects: `journeys` (the suite CI runs) and `budgets` (docs/testing.md#budgets measurements, run on
 * demand with `pnpm --filter @unfold/e2e budgets` and the release addon).
 */
export default defineConfig({
  forbidOnly: Boolean(process.env["CI"]),
  retries: 0,
  // Timing journeys (save to updated card) are measured; one app at a time keeps them honest.
  workers: 1,
  fullyParallel: false,
  reporter: process.env["CI"] ? [["list"], ["github"], ["./src/measurements-reporter.ts"]] : [["list"], ["./src/measurements-reporter.ts"]],
  outputDir: "test-results",
  expect: { timeout: 15_000 },
  projects: [
    { name: "journeys", testDir: "journeys", timeout: 180_000 },
    { name: "budgets", testDir: "budgets", timeout: 900_000 },
  ],
});
