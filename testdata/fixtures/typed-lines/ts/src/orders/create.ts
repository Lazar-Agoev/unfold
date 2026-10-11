import type { Cart, Address } from "../cart.js";
import * as models from "../cart.js";
import { reserveInventory, first } from "../inventory.js";
import { chargePayment } from "../payments.js";
import { PaymentError } from "../errors.js";
import type { User } from "../user.js";
import { legacyReport } from "../legacy.js";

export interface Order {
  id: string;
  cart: models.Cart;
  chargeId: string;
  holdId: string;
  shipTo?: Address;
  currency: models.Currency;
}

export interface Request {
  user: User;
  body: { cart: Cart };
}

export async function createOrder(req: Request): Promise<Order> {
  const cart = req.body.cart;
  const hold = await reserveInventory(cart.items);
  try {
    const charge = await chargePayment(cart, req.user);
    return toOrder(cart, charge.id, hold.id);
  } catch (error) {
    throw new PaymentError(String(error));
  }
}

function toOrder(cart: Cart, chargeId: string, holdId: string): Order {
  return { id: `order-${cart.id}`, cart, chargeId, holdId, currency: models.Currency.Usd };
}

export function describe(order: Order, format: (order: Order) => string = summary): string {
  const item = first(order.cart.items);
  notify(order, summary);
  return format(order) + (item ? ` (${item.sku})` : "") + legacyReport(order.cart, order.cart.owner);
}

function summary(order: Order): string {
  return `${order.id}: ${order.chargeId}`;
}

function notify(order: Order, render: (order: Order) => string, ...channels: string[]): void {
  render(order);
  channels.forEach((channel) => channel.trim());
}
