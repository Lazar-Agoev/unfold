// Calls, constructions, optional chains, and tagged templates (a call of its tag, with no arguments).
const first = new Constructor;
const second = new namespace.Constructor(argument(), ...rest());
new Factory().method();
new (choose())(argument());
receiver?.method?.(argument()); receiver?.[key()]?.();
tag`before${nested()}after`;
namespace.tag<Type>`value=${value()}`;
items.map((item) => transform(item))[0].done();
