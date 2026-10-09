export {};
function assert(value: unknown): asserts value {}
class C { m() {} run() { assert(this); this.m(); } }