import { handler as renamed, arrow, overloaded, Job, Handlers } from "./barrel.js";
declare function register(...handlers: unknown[]): void;
register(renamed, (arrow)!, Job, overloaded);
new Job(renamed);
const local = new Handlers();
register(local.member, local["member"], Handlers.ready, local.field);
