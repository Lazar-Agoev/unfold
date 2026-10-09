import { Worker } from "bullmq";
import { connection } from "../config.ts";
import { processOrder } from "../jobs/process-order.ts";
import { logger } from "../lib/logger.ts";
import type { OrderJob } from "../types.ts";

export const ordersWorker = new Worker<OrderJob>("orders", processOrder, { connection });
ordersWorker.on("error", (error) => logger.error("Order worker failed", error));
