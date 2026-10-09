import express, { type Request, type Response, type Router } from "express";
import { asyncHandler } from "../lib/async-handler.ts";
import { slack } from "../lib/slack.ts";
import { ordersQueue } from "../queues/queues.ts";
import { orderService } from "../services/order-service.ts";
import type { OrderDraft } from "../types.ts";
import { requireUser, validateBody } from "./middleware.ts";

export const ordersRouter: Router = express.Router();

export function listOrders(_req: Request, res: Response): void {
  res.json({ orders: orderService.list() });
}

export async function getOrder(req: Request, res: Response): Promise<void> {
  const { id } = req.params;
  const order = typeof id === "string" ? orderService.get(id) : undefined;
  if (!order) {
    res.status(404).json({ error: "Order not found" });
    return;
  }
  res.json(order);
}

export async function createOrder(req: Request, res: Response): Promise<void> {
  const draft: OrderDraft = req.body;
  if (req.header("prefer") === "respond-async") {
    const job = await ordersQueue.add("place-order", { order: draft });
    res.status(202).json({ jobId: job.id });
    return;
  }
  const order = orderService.place(draft);
  slack.post("#orders", `Order ${order.id} placed for ${order.customerId}`);
  res.status(201).json(order);
}

export function cancelOrder(req: Request, res: Response): void {
  const { id } = req.params;
  const order = typeof id === "string" ? orderService.cancel(id) : undefined;
  if (!order) {
    res.status(404).json({ error: "Order not found" });
    return;
  }
  res.json(order);
}

ordersRouter.get("/", listOrders);
ordersRouter.get("/:id", asyncHandler(getOrder));
ordersRouter.post("/", requireUser, validateBody, createOrder);
ordersRouter.delete("/:id", [requireUser, cancelOrder]);
