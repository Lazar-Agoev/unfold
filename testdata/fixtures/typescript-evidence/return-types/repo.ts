export class Repo { save() {} }
export class Other { save() {} }
export function plain(): Repo { return new Repo(); }
export function maybe(): Repo | null { return null; }
export function missing(): undefined | (Repo) { return undefined; }
export async function fetchRepo(): Promise<Repo> { return new Repo(); }
export async function fetchMaybe(): Promise<Repo | undefined> { return undefined; }
export const arrow = async (): Promise<Repo | null> => new Repo();
export function either(): Repo | Other { return new Repo(); }
export function generic<T>(value: T): T { return value; }
export function promised(): Promise<Repo> { return Promise.resolve(new Repo()); }
export class Box<T> { value?: T; save() {} }
export function boxed(): Box<string> { return new Box(); }
export function numbers(): number[] { return []; }
export function listed(): Array<Repo> { return []; }
export function table(): Map<string, Repo> { return new Map(); }
export function readonlyRows(): ReadonlyArray<number> { return []; }
export async function fetchBox(): Promise<Box<number> | null> { return null; }
export class Service {
  factory(): Repo { return new Repo(); }
  async load(): Promise<Repo> { return new Repo(); }
  run() { this.factory().save(); const made = this.factory(); made.save(); }
  async start() { (await this.load()).save(); }
}
