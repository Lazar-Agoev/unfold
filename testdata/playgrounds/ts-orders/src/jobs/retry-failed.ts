import { withRetry } from "../lib/retry.ts";
import { flushOutbox } from "./send-email.ts";

export async function retryFailed(): Promise<void> {
  await withRetry(flushOutbox, 3);
}
