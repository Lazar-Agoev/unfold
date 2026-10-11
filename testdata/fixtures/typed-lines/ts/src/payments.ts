import Stripe from "stripe";
import type { Cart, Money } from "./cart.js";
import type { User } from "./user.js";
import { CardDeclinedError } from "./errors.js";

export interface Charge {
  id: string;
  amount: Money;
  intent: Stripe.PaymentIntent;
}

const stripe = new Stripe("sk_test_typed_lines");

export async function chargePayment(cart: Cart, user: User): Promise<Charge> {
  const intent = await stripe.paymentIntents.create({ amount: total(cart), customer: user.id });
  if (intent.status !== "succeeded") {
    throw new CardDeclinedError(intent.id);
  }
  return { id: intent.id, amount: intent.amount, intent };
}

export function total(cart: Cart): Money {
  return cart.items.reduce((sum, item) => sum + item.price * item.quantity, 0);
}
