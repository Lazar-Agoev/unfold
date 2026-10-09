import { randomUUID } from "node:crypto";
import { openDatabase, type OrderRepository } from "../db/index.ts";
import { validateOrder } from "../lib/validate.ts";
import { type Order, ORDER_STATUSES } from "../types.ts";
import { events } from "./events.ts";
import * as payments from "./payment.ts";
import type { PaymentProvider } from "./payment.ts";
import "./notifier.ts";

export class OrderService {
  constructor(private repo: OrderRepository, private payments: PaymentProvider) {}

  place(input: unknown): Order {
    const draft = validateOrder(input);
    this.payments.charge(draft);
    const order: Order = {
      ...draft,
      id: randomUUID(),
      status: ORDER_STATUSES[0],
      createdAt: new Date().toISOString(),
    };
    this.repo.insert(order);
    events.emit("order.placed", order);
    return order;
  }

  list(): Order[] {
    return this.repo.list();
  }

  get(id: string): Order | undefined {
    return this.repo.find(id);
  }

  cancel(id: string): Order | undefined {
    return this.repo.cancel(id);
  }

  remove(id: string): boolean {
    return this.repo.remove(id);
  }

  get count(): number {
    return this.repo.list().length;
  }
}

export const orderService = new OrderService(openDatabase(), payments.createProvider());
