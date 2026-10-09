import { Worker } from "bullmq";
function handle() {}
new Worker("jobs", handle);
