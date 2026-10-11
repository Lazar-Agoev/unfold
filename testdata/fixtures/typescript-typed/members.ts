// Members of interfaces, of a type alias's whole object type literal, of enums, and of classes.
interface Everything<T> {
  name: string;
  "quoted-key"?: number;
  [Symbol.iterator](): Iterator<T>;
  readonly [key: string]: unknown;
  method?<U>(a: U, b: T): U;
  (call: number): string;
  new (value: string): Everything<T>;
  get size(): number;
}
type Shape = { kind: "circle"; radius: number; area(): number };
type NotMembers = { a: string } | null;
enum Color { Red, Green = "green", Blue = compute(), ["Computed"] = 4 }
const key = "k";
class Model extends Base {
  id: string;
  count = 0;
  optional?: Date;
  readonly fixed: number = 1;
  declare declared: string;
  static shared = 1;
  accessor auto = "a";
  static accessor sharedAuto = 1;
  #secret: string = "";
  [key]: number;
  constructor(public owner: User, protected readonly token = "", plain: number) {
    super();
  }
  method(): void {}
}
abstract class Figure {
  abstract kind: string;
}
