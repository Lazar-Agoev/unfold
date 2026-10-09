export const ORDER_STATUSES = ["pending", "paid", "cancelled"] as const;
export type OrderStatus = typeof ORDER_STATUSES[number];

export interface OrderItem {
  sku: string;
  quantity: number;
  price: number;
}

export interface OrderDraft {
  customerId: string;
  email: string;
  items: OrderItem[];
}

export interface Order extends OrderDraft {
  id: string;
  status: OrderStatus;
  createdAt: string;
}

export interface OrderJob { order: OrderDraft }
export interface EmailJob { to: string; subject: string; body: string }
