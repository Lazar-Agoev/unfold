export {};
class C { m() {} }
class D extends C {}
function isD(value: unknown): value is D { return value instanceof D; }
function check(c: C, other: C, third: C) {
  if (c instanceof D) { c.m(); }
  if (isD(other)) other.m();
  if (unknownCall(third)) third.m();
}
declare function unknownCall(value: unknown): boolean;
