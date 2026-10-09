import { Repo, makeRepo } from "./repo.js";
import type { Repo as RepoType } from "./repo.js";
export function use(r: Repo, t: RepoType, list: Repo[], m: Map<string, Repo>) {
  r.save(); t.save(); list.map(x => x); list[0].save(); m.get("a");
  makeRepo().save();
  new Repo().save();
  const local = new Repo(); local.save();
  (r as unknown as Other).other();
}
class Other { other() {} }
type Alias = Repo;
function aliased(a: Alias) { a.save(); }
