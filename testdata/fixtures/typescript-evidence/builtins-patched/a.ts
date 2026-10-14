export function patched(rows: number[], n: number): void {
  JSON.parse("{}");
  globalThis.JSON.parse("{}");
  rows.map((row) => row);
  [1].map((item) => item);
  "x".trim();
  n.toFixed(1);
  Math.max(1, 2);
}

export function untouched(rows: number[]): void {
  JSON.stringify({});
  rows.filter((row) => row);
  "x".toUpperCase();
  Object.keys({});
  Promise.resolve(1);
}

const service = {
  start(): void {},
  stop(): void {},
};
Object.assign(service, { start: () => {} });

const handlers = {
  open(): void {},
  close(): void {},
};
Object.defineProperty(handlers, "open", { value: () => {} });

export function objects(): void {
  service.start();
  service.stop();
  handlers.open();
  handlers.close();
}
