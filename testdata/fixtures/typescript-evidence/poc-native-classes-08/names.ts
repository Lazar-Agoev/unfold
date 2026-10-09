export {};
class Base { #m() {} ["literal"]() {} 1() {} run() { this.#m?.(); this["literal"]?.(); this[1](); } }
class Child extends Base { #m() {} }