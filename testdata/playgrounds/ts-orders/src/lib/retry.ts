import { logger } from "./logger.ts";

export async function withRetry<T>(fn: () => Promise<T>, attempts: number): Promise<T> {
  if (!Number.isInteger(attempts) || attempts < 1) throw new RangeError("Attempts must be a positive integer");
  let attempt = 0;
  while (attempt < attempts) {
    try {
      return await fn();
    } catch (error) {
      if (attempt + 1 === attempts) throw error;
      logger.error("Operation failed; retrying", error);
    } finally {
      attempt += 1;
    }
  }
  throw new Error("Retry attempts exhausted");
}
