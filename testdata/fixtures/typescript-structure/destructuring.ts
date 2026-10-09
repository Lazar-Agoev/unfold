// Destructuring: the whole declaration is an unnamed variable and each bound identifier a binding variable; parameters bind no variables.
const { a = f(), b: c, [k()]: d, nested: { value = g() }, ...rest } = source();
const [first = h(), , [inner], ...tail] = values();
function parameters({ parameter = p() } = {}, [item = q()] = []) {}
try {} catch ({ message = fallback() }) {}
for (const { entry = initialize() } of entries) {}
