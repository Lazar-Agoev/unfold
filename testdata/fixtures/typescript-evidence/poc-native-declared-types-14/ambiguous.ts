export {};
class C { m() {} }
class D { m() {} }
function call(c: C, d: D, union: C | D, flag: boolean) { union.m(); (flag ? c : d).m(); }