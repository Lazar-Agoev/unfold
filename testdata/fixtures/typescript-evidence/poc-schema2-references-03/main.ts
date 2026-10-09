function register(...args: unknown[]) {}
function named() {}
function wrap(callback: unknown) { return callback; }
class Named { constructor() {} }
function owner(callback: () => void) {
  register(named, () => {}, function () {}, Named, class {}, wrap(named), {}, callback);
}
