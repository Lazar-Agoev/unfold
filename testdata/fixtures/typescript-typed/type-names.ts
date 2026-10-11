// Which written type names are type links, and how single-file names resolve (a value never hides a type).
interface Local { a: number }
type Alias = Local;
function keywords(a: string, b: number, c: boolean, d: void, e: any, f: unknown, g: never, h: null, i: undefined, j: object, k: bigint, l: symbol): void {}
function literals(a: "x", b: 1, c: true, d: `t${string}`, e: -1) {}
function queries(a: typeof keywords, b: import("./m").T<Local>, c: keyof Local, d: Local["a"]): Local[] {
  return [];
}
function generic<T extends Local, U>(a: T, b: U[], c: Map<T, Local>): T {
  return a;
}
type Mapped<T> = { [K in keyof T]: T[K] extends Local ? K : never };
type Unwrap<T> = T extends Promise<infer U> ? U : Fallback;
type Fn = <V>(value: V, other: Local) => V;
type Ctor = new (value: Local) => Local;
interface Generic<T> {
  item: T;
  list: Array<T>;
  method<M>(m: M): T;
}
const annotated: Local | Alias = { a: 1 };
const tuple: [Local, ...Alias[], Local?] = [{ a: 1 }];
const qualified: Name.Space.Type = 1;
const day: Date = new Date();
const utility: Partial<Local> = {};
class Self<S> {
  me(): this {
    return this;
  }
  other(value: S): Self<S> {
    return this;
  }
}
function shadowed() {
  const Promise = 1;
  let p: Promise<number>;
}
