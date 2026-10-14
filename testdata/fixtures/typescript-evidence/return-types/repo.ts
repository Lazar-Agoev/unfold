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
export class Service {
  factory(): Repo { return new Repo(); }
  async load(): Promise<Repo> { return new Repo(); }
  run() { this.factory().save(); const made = this.factory(); made.save(); }
  async start() { (await this.load()).save(); }
}
