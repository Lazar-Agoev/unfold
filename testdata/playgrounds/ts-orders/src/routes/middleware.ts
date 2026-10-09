import type { NextFunction, Request, Response } from "express";
import { validateOrder } from "../lib/validate.ts";

export function requireUser(req: Request, res: Response, next: NextFunction): void {
  if (!req.header("x-user-id")) {
    res.status(401).json({ error: "Provide an x-user-id header" });
    return;
  }
  next();
}

export function validateBody(req: Request, res: Response, next: NextFunction): void {
  try {
    req.body = validateOrder(req.body);
  } catch (error) {
    const message = error instanceof Error ? error.message : "Invalid order";
    res.status(400).json({ error: message });
    return;
  }
  next();
}
