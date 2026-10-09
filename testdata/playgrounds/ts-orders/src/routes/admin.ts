import type { Express, Request, Response } from "express";
import { orderService } from "../services/order-service.ts";

export function adminStats(_req: Request, res: Response): void {
  const orders = orderService.list();
  res.json({
    orders: orderService.count,
    pending: orders.filter(order => order.status === "pending").length,
  });
}

export function registerAdminRoutes(app: Express): void {
  app.get("/admin/stats", adminStats);
}
