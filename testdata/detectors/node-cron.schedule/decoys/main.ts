import cron from "node-cron"; import * as tasks from "node-cron"; import { schedule as real, createTask } from "node-cron";
import wrong from "other-cron"; import { schedule as local } from "./local.js"; import { schedule as missing } from "./missing.js";
function handle() {} function schedule(...args: unknown[]) {}
schedule("local", handle); local("relative", handle); missing("missing", handle); wrong.schedule("wrong", handle);
cron("default-call", handle); tasks("namespace-call", handle); createTask("other", handle);
cron["schedule"]("element", handle); cron[key]("dynamic", handle); new real("constructor", handle);
const alias = real; alias("alias", handle); function shadow(cron: any) { cron.schedule("shadow", handle); }
real("real", handle);
