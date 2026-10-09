export {};
interface Interface { m(): void; }
abstract class Abstract { abstract m(): void; }
function call(value: Interface, abstract: Abstract, callback: () => void, anyValue: any, unknownValue: unknown) {
    value.m(); abstract.m(); callback(); anyValue.m(); unknownValue.m(); missing.m();
}