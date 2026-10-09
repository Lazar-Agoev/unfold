export {};
interface Shape { m(): void; }
class C { m() {} }
const value: Shape = new C(); value.m();