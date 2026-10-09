function original() {}
class Base {
  constructor() {}
  run() {}
  static staticRun() {}
  ["literal"]() {}
  get value() { return 1; }
  field = () => {};
}
class Derived extends Base { run() {} }
const obj = { method() {}, arrow: () => {}, ["literal"]: function () {}, alias: original, data: 1, duplicate() {}, duplicate() {} };
let mutable = { method() {} };
