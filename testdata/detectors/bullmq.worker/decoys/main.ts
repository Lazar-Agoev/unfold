import { Worker as Real, Queue, QueueEvents, FlowProducer } from "bullmq";
import * as bull from "bullmq"; import OtherWorker from "bullmq"; import { Worker as Wrong } from "not-bullmq";
import { Worker as Local } from "./local.js"; import { Worker as Missing } from "./missing.js";
function handle() {} class Worker { constructor(...args: unknown[]) {} }
new Worker("local", handle); new Local("relative", handle); new Missing("missing", handle);
new Wrong("wrong", handle); new Queue("queue", handle); new QueueEvents("events", handle); new FlowProducer("flow", handle);
new bull.Worker("namespace", handle); new OtherWorker("default", handle); Real("call", handle);
const Alias = Real; new Alias("alias", handle); function shadow(Real: any) { new Real("shadow", handle); }
new Real("real", handle);
