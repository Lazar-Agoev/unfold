import { Worker } from "bullmq";
import { connection } from "../config.ts";
import { sendEmail } from "../jobs/send-email.ts";
import { logger } from "../lib/logger.ts";
import { withRetry } from "../lib/retry.ts";
import type { EmailJob } from "../types.ts";

export const emailWorker = new Worker<EmailJob>("emails", async (job) => {
  await withRetry(() => sendEmail(job.data), 3);
}, { connection });
emailWorker.on("error", (error) => logger.error("Email worker failed", error));
