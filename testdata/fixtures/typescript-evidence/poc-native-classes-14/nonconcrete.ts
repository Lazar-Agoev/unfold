export {};
function actual() {}
abstract class Abstract { abstract m(): void; run() { this.m(); } }
class C { get value() { return actual; } run() { this.value(); } }