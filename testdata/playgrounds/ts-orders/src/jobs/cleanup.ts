import { orderService } from "../services/order-service.ts";
import { logger } from "../lib/logger.ts";

export default function cleanupExpiredOrders(): void {
  const orders = orderService.list();
  const expiresBefore = Date.now() - 24 * 60 * 60 * 1000;
  const purgeBefore = expiresBefore - 6 * 24 * 60 * 60 * 1000;
  const expired: string[] = [];
  for (let index = 0; index < orders.length; index++) {
    const order = orders[index]!;
    const createdAt = Date.parse(order.createdAt);
    if (order.status === "pending" && createdAt < expiresBefore) {
      orderService.cancel(order.id);
    }
    if (order.status !== "paid" && createdAt < purgeBefore) {
      expired.push(order.id);
    }
  }
  let removed = 0;
  if (expired.length > 0) {
    do {
      const id = expired.pop()!;
      if (orderService.remove(id)) removed++;
    } while (expired.length > 0);
  }
  logger.info("Expired orders cleaned up", { removed });
}
