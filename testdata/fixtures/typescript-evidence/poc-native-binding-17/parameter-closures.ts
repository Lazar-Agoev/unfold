export {};
function f() {}
function outer(a = () => f(), b = function () { f(); }, c = () => () => f(), d = { method() { f(); } }, e = class { method() { f(); } }) {
    const f = () => {};
    f();
}