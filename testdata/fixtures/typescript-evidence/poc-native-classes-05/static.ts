export {};
class Base { static m() {} static run() { this.m(); Base.m(); } }
class Child extends Base { static m() {} static run() { this.m(); super.m(); } }
class Inherited extends Base {}
Inherited.m(); Child.m();