export {};
class Base { m() {} run() { this.m(); } }
function base() { return Base; }
class Child extends base() { m() {} }