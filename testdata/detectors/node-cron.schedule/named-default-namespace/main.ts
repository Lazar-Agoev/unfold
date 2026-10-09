import cron from "node-cron"; import * as tasks from "node-cron"; import { schedule as every } from "node-cron";
function handle() {}
every("0 * * * *", handle, { timezone: "UTC" }); cron.schedule(`1 * * * *`, () => {}); tasks.schedule("2 * * * *", function tick() {});
(every as typeof every)("3 * * * *", handle); cron.schedule(expression, handle); every("", handle);
every("missing"); cron.schedule(); tasks.schedule("missing");
