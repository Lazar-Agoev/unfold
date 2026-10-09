import { logger } from "./logger.ts";

export function schedule(name: string, task: () => void): void {
  setImmediate(() => {
    logger.info("Calendar reminder", { name });
    task();
  });
}
