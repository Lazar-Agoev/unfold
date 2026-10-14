interface Row {
  id: string;
}

export function globals(text: string): void {
  JSON.parse(text);
  Object.keys({});
  Math.max(1, 2);
  Promise.all([]);
  String(text);
  parseInt(text);
  new Error(text);
  new Map<string, Row>().get(text);
  Array.isArray(text);
}

export function hosts(): void {
  console.log("host");
  setTimeout(() => {}, 1);
  fetch("https://example.com");
  Error.captureStackTrace({});
  Math.missing();
}

export function written(s: string, rows: Row[], list: ReadonlyArray<Row>, n: number, o: object, rows2: Array<Row>): void {
  s.trim();
  rows.map((row) => row.id);
  list.forEach((row) => row.id);
  n.toFixed(2);
  o.hasOwnProperty("id");
  rows2.filter((row) => row.id);
  s.missing();
}

export function literals(name: string): void {
  "x".trim();
  `a${name}`.toUpperCase();
  [1, 2].map((item) => item);
  (3).toFixed(1);
  /a/.test(name);
  ", ".join([]);
  "x".missing();
}

export function inferred(): void {
  const s = "x";
  s.trim();
  const rows = [1, 2];
  rows.map((item) => item);
}

export function shadowed(JSON: { parse(text: string): void }): void {
  JSON.parse("{}");
}
