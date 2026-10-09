export const PORT = Number(process.env.PORT ?? "3000");
export const API_VERSION = process.env.API_VERSION ?? "1";
export const QUEUES = { reports: process.env.REPORT_QUEUE ?? "reports" };
export const connection = {
  host: process.env.REDIS_HOST ?? "127.0.0.1",
  port: Number(process.env.REDIS_PORT ?? "6379"),
  maxRetriesPerRequest: null,
};
