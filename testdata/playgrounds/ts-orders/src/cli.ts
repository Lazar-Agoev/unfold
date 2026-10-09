#!/usr/bin/env node
import { importOrders } from "./jobs/import-orders.ts";
import { retryFailed } from "./jobs/retry-failed.ts";
import { logger } from "./lib/logger.ts";

export async function main(): Promise<void> {
  const [, , command, path] = process.argv;
  switch (command) {
    case "import":
      await importOrders(path);
      break;
    case "report": {
      const { printReport } = await import("./jobs/report.ts");
      printReport();
      break;
    }
    case "replay":
    case "retry":
      await retryFailed();
      break;
    case undefined:
    case "--help":
      logger.info("Usage: orders import [file] | report | replay | retry");
      break;
    default:
      throw new Error(`Unknown command: ${command}`);
  }
}

main().catch((error: unknown) => {
  logger.error("Command failed", error);
  process.exitCode = 1;
});
