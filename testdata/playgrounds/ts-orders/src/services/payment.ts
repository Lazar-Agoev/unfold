import { logger } from "../lib/logger.ts";
import type { OrderDraft } from "../types.ts";

export class PaymentProvider {
  charge(order: OrderDraft): void {
    const total = order.items.reduce((sum, item) => sum + item.price * item.quantity, 0);
    if (!Number.isFinite(total) || total <= 0) throw new Error("Invalid payment total");
    logger.info("Payment preview", { customerId: order.customerId, total });
  }
}

export class StripeProvider extends PaymentProvider {
  override charge(order: OrderDraft): void {
    super.charge(order);
    logger.info("Stripe payment recorded in preview mode", { customerId: order.customerId });
  }
}

export function createProvider(): PaymentProvider {
  return new StripeProvider();
}
