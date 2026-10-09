import type { OrderDraft, OrderItem } from "../types.ts";

export function validateOrder(input: unknown): OrderDraft {
  if (input === null || typeof input !== "object") {
    throw new Error("An order must be an object");
  }
  const { customerId, email, items } = input as Record<string, unknown>;
  if (typeof customerId !== "string" || customerId.trim() === "") {
    throw new Error("A customer ID is required");
  } else if (typeof email !== "string" || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
    throw new Error("A valid customer email is required");
  } else if (!Array.isArray(items) || items.length === 0) {
    throw new Error("An order must include at least one item");
  }
  const validatedItems = items.map((item: unknown): OrderItem => {
    if (item === null || typeof item !== "object") {
      throw new Error("Each item must be an object");
    }
    const { sku, quantity, price } = item as Record<string, unknown>;
    if (typeof sku !== "string" || sku.trim() === "") {
      throw new Error("Each item needs a SKU");
    } else if (typeof quantity !== "number" || !Number.isSafeInteger(quantity) || quantity <= 0) {
      throw new Error("Item quantities must be positive integers");
    } else if (typeof price !== "number" || !Number.isFinite(price) || price <= 0) {
      throw new Error("Item prices must be positive numbers");
    }
    return { sku: sku.trim(), quantity, price };
  });
  return { customerId: customerId.trim(), email: email.trim(), items: validatedItems };
}
