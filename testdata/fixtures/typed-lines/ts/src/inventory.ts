import type { CartItem } from "./cart.js";
import { OutOfStockError } from "./errors.js";

export interface Hold {
  id: string;
  items: CartItem[];
  expiresAt: Date;
}

export async function reserveInventory(items: CartItem[]): Promise<Hold> {
  if (items.length === 0) {
    throw new OutOfStockError();
  }
  return { id: "hold-1", items, expiresAt: new Date() };
}

export function first<T>(items: T[]): T | undefined {
  return items[0];
}
