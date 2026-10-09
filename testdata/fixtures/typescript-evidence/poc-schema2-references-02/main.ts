function register(...args: unknown[]) {}
class Base {
  run() {}
  safe() {}
  field = () => {};
  get accessor() { return () => {}; }
  static ready() {}
  install() { register(this.run, this.safe, this.field, this.accessor, Base.ready); }
}
class Derived extends Base {
  run() {}
  field = () => {};
  install() { register(super.run); }
}
let mutable = () => {};
var legacy = () => {};
function owner(callback: () => void) {
  register(callback, mutable, legacy);
  callback();
  (() => callback())();
  new callback();
}
function ordinary() {}
new ordinary();
