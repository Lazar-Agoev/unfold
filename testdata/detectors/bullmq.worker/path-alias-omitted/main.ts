import { Worker } from "bullmq";
import { Worker as Omitted } from "./omitted.js";
function handle() {} new Worker("local", handle);
new Omitted("omitted", handle);
