export {};
class Base { m() {} }
class Child extends Base { run() { const arrow = () => super.m(); function nested() { super.m(); } } }