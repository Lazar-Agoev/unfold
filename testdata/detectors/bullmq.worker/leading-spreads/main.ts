import { Worker } from "bullmq";
function handle() {} function wrap(fn: unknown) { return fn; }
new Worker(...args, handle);
new Worker(...args, wrap(handle));
new Worker(...args);
