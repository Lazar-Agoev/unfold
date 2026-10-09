import type { Order } from "../types.ts";

export class OrderRepository {
  private orders = new Map<string, Order>();

  insert(order: Order): Order {
    if (this.orders.has(order.id)) throw new Error(`Order ${order.id} already exists`);
    this.orders.set(order.id, order);
    return order;
  }

  find(id: string): Order | undefined {
    return this.orders.get(id);
  }

  list(): Order[] {
    return Array.from(this.orders.values());
  }

  remove(id: string): boolean {
    return this.orders.delete(id);
  }

  cancel(id: string): Order | undefined {
    const order = this.find(id);
    if (order) order.status = "cancelled";
    return order;
  }
}
