export {};
class Base { m() {} }
class Child extends Base { m() {} }
function call(base: Base, child: Child) { base.m(); child.m(); }
class Holder { constructor(private base: Base) {} run() { this.base.m(); } }