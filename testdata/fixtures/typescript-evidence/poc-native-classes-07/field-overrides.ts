export {};
class Base { m() {} run() { this.m(); } }
class Field extends Base { m!: () => void; }
class Other { n() {} run() { this.n(); } }
class Parameter extends Other { constructor(public n: () => void) { super(); } }