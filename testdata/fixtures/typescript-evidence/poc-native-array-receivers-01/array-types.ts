export {};
class Repo { save() {} }
function make(): Repo[] { return []; }
function run(a: Array<Repo>, b: ReadonlyArray<Repo>) { a[0].save(); b[0].save(); make()[0].save(); }
const constructed = new Array<Repo>(); constructed[0].save();