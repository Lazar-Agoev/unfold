import { readFile } from "node:fs/promises";
import { orderService } from "../services/order-service.ts";
import { validateOrder } from "../lib/validate.ts";
import { logger } from "../lib/logger.ts";

export async function importOrders(path: string = "orders.json"): Promise<void> {
  const input: unknown = JSON.parse(await readFile(path, "utf8"));
  if (!Array.isArray(input)) throw new Error("Orders file must contain an array");
  const failures: unknown[] = [];
  let imported = 0;
  for (const [index, value] of input.entries()) {
    try {
      const draft = validateOrder(value);
      orderService.place(draft);
      imported++;
    } catch (error) {
      logger.error(`Could not import order at row ${index + 1}`, error);
      failures.push(error);
    }
  }
  logger.info("Orders import completed", { imported, rejected: failures.length });
  if (failures.length > 0) throw new AggregateError(failures, "Some orders could not be imported");
}
