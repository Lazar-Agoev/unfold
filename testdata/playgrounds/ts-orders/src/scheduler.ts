import cron, { schedule } from "node-cron";
import cleanupExpiredOrders from "./jobs/cleanup.ts";
import { printReport, reportMetrics } from "./jobs/report.ts";
import { schedule as scheduleReminder } from "./lib/calendar.ts";

cron.schedule("0 2 * * *", cleanupExpiredOrders);
schedule("*/15 * * * *", () => reportMetrics());
scheduleReminder("scheduler startup report", printReport);
