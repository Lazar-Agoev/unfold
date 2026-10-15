class FakeMap {
  get(key: string): string {
    return key;
  }
}

class FakeArray {
  map(): number[] {
    return [];
  }
}

globalThis.Map = FakeMap as any;
(globalThis as any).Array = FakeArray;

export function useSaved(): string {
  const changedMap = new Map<string, string>();
  return changedMap.get("a") ?? "";
}

export function useArray(): number[] {
  const rows = new Array<number>(3);
  return rows.map((row) => row * 2);
}

export function chained(): string {
  return new Map<string, number>().size.toFixed();
}

export function untouched(): boolean {
  [1, 2].map((n) => n);
  return new Set<number>().has(1);
}
