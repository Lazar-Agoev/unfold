import { logger } from "../lib/logger.ts";
import type { Order } from "../types.ts";
import { events } from "./events.ts";

export abstract class Notifier {
  abstract send(order: Order): void;

  notify(order: Order): void {
    this.send(order);
  }
}

class ReceiptNotifier extends Notifier {
  override send(order: Order): void {
    logger.info("Order receipt ready", { orderId: order.id, to: order.email });
  }
}

const notifier = new ReceiptNotifier();

export function notifyCustomer(order: Order): void {
  notifier.notify(order);
}

events.on("order.placed", notifyCustomer);
