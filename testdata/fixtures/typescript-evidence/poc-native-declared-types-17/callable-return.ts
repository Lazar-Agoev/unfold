export {};
class C { m() {} }
class D { m() {} }
const factory: () => D = (): C => new C();
class Holder { factory: () => D = (): C => new C(); run() { this.factory().m(); } }
factory().m();