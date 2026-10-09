import type { Order } from "../types.ts";

const subscriptions = new Map<string, Array<(payload: Order) => void>>();

export const events = {
  on(name: string, handler: (payload: Order) => void): void {
    const handlers = subscriptions.get(name) ?? [];
    handlers.push(handler);
    subscriptions.set(name, handlers);
  },
  emit(name: string, payload: Order): void {
    const handlers = subscriptions.get(name) ?? [];
    for (const handler of handlers) handler(payload);
  },
};
