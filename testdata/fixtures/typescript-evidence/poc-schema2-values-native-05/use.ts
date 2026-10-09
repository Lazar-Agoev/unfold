class Base { method() {} } class Potential extends factory() { method() {} }
function receive(base: Base) { const refs = [base.method]; }
class Narrowed { method() {} run() { if (unknownPredicate(this)) {} const refs = [this.method]; } }
import type { A, B } from "external"; function union(value: A | B) { const refs = [value.method]; }