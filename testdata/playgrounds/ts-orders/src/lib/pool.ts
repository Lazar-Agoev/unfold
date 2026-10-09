import { logger } from "./logger.ts";

export class Worker {
  constructor(private name: string, private task: () => void) {}

  run(): void {
    logger.info("Running maintenance", { name: this.name });
    this.task();
  }
}
