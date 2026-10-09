import { schedule } from "node-cron";
function handle() {} let mutable = handle;
schedule("mutable", mutable); schedule("missing", absent);
function register(callback: () => void) { schedule("parameter", callback); }
schedule("data", "./file.js"); schedule("spread", ...handlers);
