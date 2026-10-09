import { startConsumer } from "./queue.js";
import { validateOrder } from "./validate.js";

export function processOrder(message: unknown) {
  validateOrder(message);
}

startConsumer("orders", processOrder);
