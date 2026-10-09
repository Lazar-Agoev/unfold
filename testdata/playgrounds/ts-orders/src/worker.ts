import "./queues/orders-worker.ts";
import "./queues/email-worker.ts";
import "./queues/report-worker.ts";
import { Worker } from "./lib/pool.ts";
import { orderService } from "./services/order-service.ts";

function purge(): void {
  for (const order of orderService.list()) {
    if (order.status === "cancelled") orderService.remove(order.id);
  }
}

const maintenance = new Worker("cleanup-pool", purge);
maintenance.run();
