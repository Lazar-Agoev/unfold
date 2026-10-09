export function validateOrder(order: unknown) {
  if (!order) throw new Error("empty order");
}
