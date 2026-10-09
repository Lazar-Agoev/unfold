export {};
class Base { m() {} static s() {} }
class Derived extends Base {
  run() { this.m(); const f = () => this.m(); f(); function g(this: any) { this.m(); } g(); }
  static make() { this.s(); super.s(); }
  field = this.m();
  static { this.s(); }
}
function free() { this.m(); }
const literal = { m() {}, run() { this.m(); } };
literal.run();
