function parse(text: string): unknown {
  return text;
}

function map(this: unknown[]): unknown[] {
  return [];
}

JSON.parse = parse;
(Array.prototype as any).map = map;
Object.defineProperty(String.prototype, "trim", { value: () => "" });
Reflect.set(Number.prototype, "toFixed", () => "");
(globalThis as any).Math = { max: () => 0 };
export {};
