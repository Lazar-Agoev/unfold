import type { Job } from "bullmq";
import { validateOrder } from "../lib/validate.ts";
import { emailQueue } from "../queues/queues.ts";
import { orderService } from "../services/order-service.ts";
import type { OrderJob } from "../types.ts";

export async function processOrder(job: Job<OrderJob>): Promise<void> {
  const draft = validateOrder(job.data.order);
  const order = orderService.place(draft);
  await emailQueue.add("confirmation", {
    to: order.email,
    subject: `Order ${order.id} confirmed`,
    body: `Your order contains ${order.items.length} items.`,
  });
}
