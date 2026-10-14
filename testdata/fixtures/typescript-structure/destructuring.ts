// Destructuring: a declaration is code; its defaults and computed keys are calls of the body, and the call it initializes with binds the whole pattern.
const { a = f(), b: c, [k()]: d, nested: { value = g() }, ...rest } = source();
const [first = h(), , [inner], ...tail] = values();
function parameters({ parameter = p() } = {}, [item = q()] = []) {}
try {} catch ({ message = fallback() }) {}
for (const { entry = initialize() } of entries) {}
