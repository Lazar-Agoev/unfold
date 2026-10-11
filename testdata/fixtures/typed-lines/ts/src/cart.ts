import type { User } from "./user.js";

export type CartId = string;
export type Money = number;

export interface CartItem {
  sku: string;
  quantity: number;
  price: Money;
}

export interface Cart {
  id: CartId;
  owner: User;
  items: CartItem[];
  coupon?: string;
  readonly createdAt: Date;
}

// Two declarations merge into one interface, so a link to it can't name one block.
export interface Address {
  street: string;
}

export interface Address {
  city: string;
}

export type CartSummary = { count: number; total: Money; lines: Record<string, CartItem> };

export type Discount = Partial<Cart> | null;

export enum Currency {
  Usd = "usd",
  Eur = "eur",
}
