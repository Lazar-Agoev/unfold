import { Queue } from "bullmq";
import { connection } from "../config.ts";
import type { EmailJob, OrderJob } from "../types.ts";

export const ordersQueue: Queue<OrderJob> = new Queue<OrderJob>("orders", { connection });
export const emailQueue: Queue<EmailJob> = new Queue<EmailJob>("emails", { connection });
