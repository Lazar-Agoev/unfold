// Signatures and parameters: written return types, `this`, rest, optional, default, and destructured parameters.
export function plain(a: number, b?: string, c = 1, ...rest: Array<string>): Promise<void> {
  return Promise.resolve();
}
function withThis(this: HTMLElement, event: Event): void {}
const arrow = async ({ id, name }: { id: string; name: string }, [first]: number[] = []) => id;
const expression = function (value: unknown): value is string {
  return typeof value === "string";
};
class Service {
  constructor(private readonly url: string, public retries?: number) {}
  get size(): number {
    return 0;
  }
  set size(value: number) {}
  run<T>(input: T, options: { verbose?: boolean } = {}): T {
    return input;
  }
  overloaded(value: string): string;
  overloaded(value: unknown) {
    return value;
  }
}
const literal = { method(x: number): number { return x; }, get value() { return 1; } };
function untyped(a, b) {}
