export {};
function f(value: string): void;
function f(value: number): void;
function f(value: unknown) {}
declare function ambient(): void;
f("value"); f(1); ambient();