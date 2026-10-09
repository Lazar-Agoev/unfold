import { orderService } from "../services/order-service.ts";
import { logger } from "../lib/logger.ts";

export function printReport(): void {
  const orders = orderService.list().map(({ id, customerId, status, createdAt }) => ({
    id, customerId, status, createdAt,
  }));
  logger.info("Orders report", { orders });
}

export function reportMetrics(): void {
  const orders = orderService.list();
  logger.info("Order metrics", {
    total: orderService.count,
    pending: orders.filter((order) => order.status === "pending").length,
    paid: orders.filter((order) => order.status === "paid").length,
    cancelled: orders.filter((order) => order.status === "cancelled").length,
  });
}
