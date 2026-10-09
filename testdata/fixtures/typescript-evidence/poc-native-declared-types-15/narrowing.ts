export {};
class C { m() {} }
class D { m() {} }
function call(value: C) { if (value instanceof D) value.m(); }