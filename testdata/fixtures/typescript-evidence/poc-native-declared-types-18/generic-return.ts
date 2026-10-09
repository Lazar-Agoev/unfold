export {};
class T { m() {} }
const factory = <T>(): T => null!;
factory<unknown>().m();