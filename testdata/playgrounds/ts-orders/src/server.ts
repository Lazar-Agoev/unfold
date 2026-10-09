import express, { type Express, type Request, type Response } from "express";
import { API_VERSION, PORT } from "./config.ts";
import { logger } from "./lib/logger.ts";
import { emailQueue, ordersQueue } from "./queues/queues.ts";
import { registerAdminRoutes } from "./routes/admin.ts";
import { healthRouter } from "./routes/health.ts";
import { ordersRouter } from "./routes/orders.ts";
import { orderService } from "./services/order-service.ts";

export const app: Express = express();
app.use(express.json());
app.get("/health", healthCheck);
app.get(`/v${API_VERSION}/status`, status);
app.use("/orders", ordersRouter);
app.use("/health", healthRouter);
registerAdminRoutes(app);

const server = app.listen(PORT, () => {
  logger.info("HTTP server listening", { port: PORT });
});

export function healthCheck(_req: Request, res: Response): void {
  res.json({ status: "ok" });
}

export function status(_req: Request, res: Response): void {
  res.json({ version: API_VERSION, orders: orderService.count });
}

export function shutdown(): void {
  server.close(error => {
    if (error) {
      logger.error("HTTP shutdown failed", error);
      process.exitCode = 1;
    }
    void Promise.all([ordersQueue.close(), emailQueue.close()])
      .then(() => logger.info("HTTP server stopped"))
      .catch(error => {
        logger.error("Queue shutdown failed", error);
        process.exitCode = 1;
      });
  });
}

process.on("SIGTERM", shutdown);
