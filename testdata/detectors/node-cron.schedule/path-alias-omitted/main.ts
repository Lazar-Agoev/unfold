import { schedule } from "node-cron";
import { schedule as missingSchedule } from "./omitted.js";
function handle() {} schedule("local", handle);
missingSchedule("omitted", handle);
