import { Base } from "some-lib";

interface Row {
  id: string;
}

class Rows extends Array<Row> {
  map<U>(fn: (row: Row) => U): U[] {
    return [];
  }
}

class Store implements Map<string, number> {
  get(key: string): number | undefined {
    return undefined;
  }
}

interface Bag extends Set<number> {}

class Bucket implements Bag {
  has(value: number): boolean {
    return false;
  }
}

class Failure extends AggregateError {
  toString(): string {
    return "failure";
  }
}

class Text extends String {
  trim(): string {
    return "";
  }
}

class Remote extends Base {
  forEach(): void {}
}

class Counter implements Iterable<number> {
  [Symbol.iterator](): Iterator<number> {
    return [1].values();
  }
  slice(): void {}
}

export function written(rows: Row[], map: Map<string, number>, bag: Set<number>, error: Error, text: String, s: string): void {
  rows.map((row) => row.id);
  rows.filter((row) => row.id);
  rows.forEach((row) => row.id);
  rows.slice();
  map.get("a");
  map.has("a");
  bag.has(1);
  bag.add(1);
  error.toString();
  text.trim();
  s.trim();
}

export function protocols(
  object: object,
  value: Object,
  fn: Function,
  iterator: Iterator<number>,
  thenable: PromiseLike<number>,
  list: ReadonlyArray<Row>,
  promise: Promise<number>,
): void {
  object.toString();
  value.toString();
  fn.call(null);
  iterator.next();
  thenable.then((n) => n);
  list.map((row) => row.id);
  promise.then((n) => n);
}

export function exact(): void {
  [1, 2].map((n) => n);
  new Map<string, number>().get("a");
  "x".trim();
}

export function use(): void {
  written(new Rows(), new Store() as any, new Bucket() as any, new Failure([]), new Text(""), "");
  new Remote().forEach();
  new Counter().slice();
}

class Listing extends Array<number> {
  static from(): Listing {
    return new Listing();
  }
}

export function queries(arrays: typeof Array, json: typeof JSON): void {
  arrays.from([]);
  json.parse("{}");
  new Listing();
}

export function arrayLike(items: ArrayLike<number>, iterable: Iterable<number>): void {
  items.toString();
  iterable[Symbol.iterator]();
}
