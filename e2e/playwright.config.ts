import { defineConfig } from "@playwright/test";

/**
 * The journeys launch the built, unpackaged app (`ui/dist`, `desktop/out`, and the addon in
 * `crates/unfold-node`); `just e2e` builds them first. Each test gets its own user-data folder,
 * home, and copies of the playgrounds, so tests never share state.
 */
export default defineConfig({
  testDir: "journeys",
  timeout: 180_000,
  forbidOnly: Boolean(process.env["CI"]),
  retries: 0,
  // The agents' MCP server listens on a fixed port, so one app runs at a time.
  workers: 1,
  fullyParallel: false,
  reporter: process.env["CI"] ? [["list"], ["github"]] : [["list"]],
  outputDir: "test-results",
  expect: { timeout: 15_000 },
});
