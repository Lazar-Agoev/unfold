import create from "remote-factory";
import type { Client } from "remote-types";
import type * as types from "remote-types";
import { shared } from "./factory.js";
function inspect(...args: unknown[]) {}
const first = create();
const second = first();
const third = second();
let mutable = create();
const alias = create;
const cycleA = cycleB();
const cycleB = cycleA();
function localCreate() { return {}; }
const local = localCreate();
declare let ambient: Client;
function owner(annotated: Client, namespace: types.Client) {
  inspect(create, shared, first, second, third, mutable, alias, cycleA, local, annotated, namespace, create.a.b, create.a.b.c, ambient);
}
