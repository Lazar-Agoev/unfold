export {};
class C { m() {} }
class D { m() {} }
function factory(): C { return new C(); }
(factory as () => D)().m();