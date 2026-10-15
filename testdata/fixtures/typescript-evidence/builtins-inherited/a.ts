import { Base } from "some-lib";

export class Rows extends Array<number> {
  map<U>(fn: (value: number, index: number, array: number[]) => U): U[] {
    return super.map(fn);
  }

  total(): number {
    return this.reduce((sum, value) => sum + value, 0);
  }

  firstEven(): number | undefined {
    return this.find((value) => value % 2 === 0);
  }

  static copy(): number[] {
    return super.from([1]);
  }
}

export class Sorted extends Rows {
  find(): number | undefined {
    return super.find((value) => value > 1);
  }
}

export class Registry extends Map<string, number> {
  get(key: string): number | undefined {
    return super.get(key);
  }

  describe(): string {
    return super.toString();
  }
}

export class Failure extends Error {
  trace(): void {
    Error.captureStackTrace(this);
  }

  describe(): string {
    return super.toString();
  }
}

export class Remote extends Base {
  call(): void {
    this.send();
    this.close();
  }
}

export class Retrying extends Remote {
  send(): void {}
}

export function use(rows: Rows, registry: Registry, remote: Remote): void {
  rows.push(1);
  rows.find((value) => value > 0);
  registry.set("a", 1);
  Rows.from([2]);
  new Rows().slice();
  remote.close();
  remote.send();
}
