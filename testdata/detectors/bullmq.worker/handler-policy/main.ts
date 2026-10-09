import { Worker } from "bullmq"; import { outside } from "handlers";
function handle() {} function wrap(fn: unknown) { return fn; } class Handler {}
new Worker("wrapper", wrap(handle)); new Worker("class", Handler);
new Worker("external", outside); new Worker("spread", ...handlers);
