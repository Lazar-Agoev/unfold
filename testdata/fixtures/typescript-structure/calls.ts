// Calls, constructions, optional chains, tagged templates (a call of its tag, with no arguments),
// and callbacks (named by the property they are written as, when they are one).
const first = new Constructor;
const second = new namespace.Constructor(argument(), ...rest());
new Factory().method();
new (choose())(argument());
receiver?.method?.(argument()); receiver?.[key()]?.();
tag`before${nested()}after`;
namespace.tag<Type>`value=${value()}`;
items.map((item) => transform(item))[0].done();
useMutation({ onSuccess: (data) => refresh(data), onError(error) { report(error); }, retry: wrap(() => again()) });
