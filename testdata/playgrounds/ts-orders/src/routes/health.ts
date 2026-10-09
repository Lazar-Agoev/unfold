import { Router, type Router as ExpressRouter } from "express";

export const healthRouter: ExpressRouter = Router();

healthRouter.get("/live", (_req, res) => {
  res.json({ status: "alive", uptime: process.uptime() });
});
