function named() {}
function register(...args: unknown[]) {}
function tag(parts: TemplateStringsArray, ...args: unknown[]) {}
const obj = { method() {} };
const key = "method";
register(() => {}, function () {}, class {}, [named], { named }, ...[named], obj[key], named as () => void);
tag`hello ${named}`;
