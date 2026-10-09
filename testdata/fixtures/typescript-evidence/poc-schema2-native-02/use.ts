import type { Client as A } from "client-a";
import { Client as B } from "client-b";
function run(a: A, b: B, both: A | B, array: number[]) { a.send(); b.send(); both.send(); array.map(x => x); }
class Derived extends B { execute() { this.send(); super.send(); } }
console.log("ready");