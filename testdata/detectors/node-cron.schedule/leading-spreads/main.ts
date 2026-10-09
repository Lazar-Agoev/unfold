import { schedule } from "node-cron";
function handle() {}
schedule(...args, handle);
schedule(...args, () => {});
schedule(...args);
