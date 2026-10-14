// Names of anonymous functions and classes come from the declaration, class property, or object property they initialize, or from `export default`.
const variable = function () {};
const explicit = function named() {};
let target;
target = () => {};
target ||= class {};
holder.property = () => {};
function withDefault(callback = () => {}) {}
const { binding = () => {} } = source;
const object = { property: () => {}, [computed]: () => {}, 42: () => {}, "text": () => {}, method() {}, get value() { return 1; } };
class Fields { field = () => {}; static #secret = function () {}; [dynamic] = () => {}; constructor() {} }
const wrapped = ((() => {}) as Handler)!;
export default (() => {});
[assignedDefault = () => {}] = pairs;
({ key: renamed = function () {}, shorthand = () => {} } = options);
