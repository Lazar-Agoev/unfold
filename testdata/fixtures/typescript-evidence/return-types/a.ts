import { plain, maybe, missing, fetchRepo, fetchMaybe, arrow, either, generic, promised, Repo, Box, boxed, numbers, listed, table, readonlyRows, fetchBox } from "./repo.js";
export async function use() {
  const a = plain(); a.save(); a.absent();
  let b = maybe(); b!.save();
  const c = missing(); c!.save();
  plain().save();
  const d = await fetchRepo(); d.save();
  (await fetchRepo()).save();
  const e = await fetchMaybe(); e!.save();
  const f = await arrow(); f!.save();
  const g = either(); g.save();
  const h = generic(new Repo()); h.save();
  const pending = fetchRepo(); pending.then();
  const i = await promised(); i.save();
  const j = await plain(); j.save();
  boxed().save();
  const k: Box<string> = boxed(); k.save();
  numbers().map((n) => n);
  listed().filter((row) => row);
  table().get("a");
  readonlyRows().map((n) => n);
  promised().then();
  (await fetchBox())!.save();
}
