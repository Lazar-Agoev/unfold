// Heritage clauses: each base is an `extends` or `implements` type link, and the type names in its
// type arguments are ordinary links. A class's `extends` names a type only as a name or member chain.
interface Base<T> { value: T }
interface Named { name: string }
interface Both<T> extends Base<T>, Named, Missing<T> {}
class Root<R> { root!: R }
class Leaf<L> extends Root<L> implements Named, Base<Leaf<L>> {
  name = "";
  value!: Leaf<L>;
}
class Mixed extends mixin(Root) {}
class Failure extends Error {}
const made = class extends Root<number> {};
declare class Ambient extends Root<string> {}
namespace ns {
  export class Inner {}
}
class Qualified extends ns.Inner {}
function mixin<C>(base: C): C {
  return base;
}
