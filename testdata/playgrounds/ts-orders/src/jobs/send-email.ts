import { logger } from "../lib/logger.ts";
import type { EmailJob } from "../types.ts";

const outbox = new Set<EmailJob>();

export async function sendEmail(payload: EmailJob): Promise<void> {
  if (!payload.to.includes("@")) throw new Error("Email recipient is invalid");
  outbox.add(payload);
  await new Promise<void>((resolve) => setTimeout(resolve, 10));
  logger.info("Email delivered", payload);
  outbox.delete(payload);
}

export async function flushOutbox(): Promise<void> {
  for (const payload of [...outbox]) await sendEmail(payload);
}
