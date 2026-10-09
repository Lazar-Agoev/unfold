export {};
class Plain {}
class Base { constructor() {} }
class Child extends Base { constructor() { super(); } }
new Plain; new Base(); new Child();