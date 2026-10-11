import type Stripe from "stripe";

import type { Address, CartItem } from "../cart.js";

export interface Timestamped<T> {
  createdAt: T;
}

export interface Tracked extends Timestamped<Date> {
  updatedAt?: Date;
}

/** What a customer submits. */
export interface OrderDraft {
  customerId: string;
  email: string;
  items: CartItem[];
}

// Extends with generics. `Timestamped` is reached twice, directly and through `Tracked`.
export interface Order extends OrderDraft, Timestamped<Date>, Tracked {
  id: string;
  status: "open" | "paid";
}

// `Address` merges two declarations, so no one block is its base; `Stripe.MetadataParam` comes from a library.
export interface Shipment extends Address, Stripe.MetadataParam {
  trackingId: string;
}

export abstract class Entity<K> {
  id!: K;
}

export interface Auditable {
  auditedBy?: string;
}

export class OrderRecord extends Entity<string> implements OrderDraft, Auditable {
  customerId = "";
  email = "";
  items: CartItem[] = [];
  auditedBy?: string;
}
