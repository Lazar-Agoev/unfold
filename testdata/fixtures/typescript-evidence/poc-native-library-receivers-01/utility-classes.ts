export {};
class Repo { save() {} }
function use(a: Pick<Repo, "save">, b: Readonly<Repo>, c: Partial<Repo>, d: Record<string, Repo>, e: Awaited<Repo>) {
    a.save(); b.save(); c.save?.(); d["value"].save(); e.save();
}