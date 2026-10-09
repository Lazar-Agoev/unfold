export {};
class C { m() {} }
class D { m() {} }
function call(value: C) { (value as D).m(); (<D>value).m(); }