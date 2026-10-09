export function over(a: string): void;
export function over(a: number): void;
export function over(a: any) {}
over(1);
class K { m(a: string): void; m(a: any) {} constructor(); constructor(a?: any) {} }
new K().m("x");
new K();
declare function ambient(): void;
ambient();
abstract class Abs { abstract go(): void; run() { this.go(); } }
