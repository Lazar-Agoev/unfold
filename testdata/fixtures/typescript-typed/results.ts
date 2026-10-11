// Where each call's value lands, through parentheses, await, `!`, `as`, `satisfies`, and `<T>`.
type T = unknown;
async function results() {
  const a = f();
  const b = await g();
  const c = (await (h()!)) as T;
  const d = (<T>i()) satisfies T;
  let e;
  e = j();
  k();
  await l();
  outer(inner(), ...spread(), (paren()));
  new Outer(m());
  obj.method().chain();
  if (cond()) {
  }
  return n();
}
const arrowBody = () => o();
const arrowAwait = async () => await p();
function* gen() {
  yield q();
}
const { x } = r();
const [y = s()] = t;
for (const item of u()) {
}
tag`x${v()}`;
function thrower() {
  throw make();
}
