import { Worker as Consumer } from "bullmq"; import { handle } from "./handlers.js";
new Consumer("orders", handle, { concurrency: 2 });
new (Consumer as typeof Consumer)(`reports`, () => {});
new Consumer(queueName, handle); new Consumer("", handle);
new Consumer("a\nb", handle); new Consumer("missing"); new Consumer();
