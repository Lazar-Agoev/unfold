export {};
class Base<T> { constructor() {} }
class Child extends Base<string> { constructor() { super(); } }
namespace N { export class Base<T> { constructor() {} } }
class Other extends N.Base<number> { constructor() { super(); } }