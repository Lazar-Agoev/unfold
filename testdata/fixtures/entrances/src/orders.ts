import { validateOrder } from "./validate.js";

export function createOrder(request: { body: unknown }) {
  validateOrder(request.body);
}
