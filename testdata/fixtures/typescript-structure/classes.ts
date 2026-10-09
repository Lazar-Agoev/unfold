// Classes and members: decorators, heritage, computed names, parameter properties, static blocks, field initializers, overloads, private names, export default, abstract and ambient classes.
@decorate(configuration())
export class Service extends base() implements Shape {
    constructor(@inject() private dependency: Dependency = construct()) { super(argument()); }
    @decorateMember()
    [methodName()]() { return body(); }
    static initialized = initialize();
    #private = value();
    get field() { return getter(); }
    set field(value: number) { setter(value); }
    static { setup(); }
    handler = () => run();
    overloaded(): void;
    overloaded() {}
    #hidden() {}
    [index: string]: unknown;
}
export default class extends Base<T> {}
abstract class Abstract { abstract area(): number; }
declare class Ambient { method(): void; }
const Anonymous = class { method() {} };
