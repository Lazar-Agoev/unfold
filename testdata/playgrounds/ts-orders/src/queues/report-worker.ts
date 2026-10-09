import { Worker } from "bullmq";
import { URL } from "node:url";
import { connection, QUEUES } from "../config.ts";
import { logger } from "../lib/logger.ts";

export const reportWorker = new Worker(QUEUES.reports, new URL("../../report-processor.js", import.meta.url), { connection });
reportWorker.on("error", (error) => logger.error("Report worker failed", error));
