export {};
class Base { #m() {} run(value: Child) { value.#m(); } }
class Child extends Base { #m() {} }