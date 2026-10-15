function fake(): string {
  return "fake";
}

export function patchMap(): string {
  const map = new Map<string, string>();
  map.get = fake;
  return map.get("a") ?? "";
}

export function patchRows(): number[] {
  const rows: number[] = [1, 2];
  (rows as any).map = fake;
  return rows.map((row) => row * 2);
}

export function patchError(error: TypeError): void {
  error.toString = fake;
}

export function others(map: Map<string, string>, rows: number[], error: Error, failure: TypeError): void {
  map.get("b");
  map.has("b");
  rows.filter((row) => row > 0);
  error.toString();
  failure.toString();
  new Error("x").toString();
}

class Registry extends Map<string, number> {}

export function patchRegistry(registry: Registry): void {
  registry.delete = () => false;
  registry.delete("a");
  registry.clear();
}

export function exact(): void {
  new Map<string, number>().delete("a");
}

class Stack extends Array<number> {
  clear(index: number): void {
    this[index] = 0;
  }
}

export function elements(rows: number[], index: number): number[] {
  rows[index] = 1;
  return rows.slice();
}

export function patchSet(set: Set<number>, key: string): boolean {
  (set as any)[key] = fake;
  return set.has(1);
}
