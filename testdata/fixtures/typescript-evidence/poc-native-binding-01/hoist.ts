export {};
function f() {}
function outer() { f(); { var f = () => {}; } f(); }
function other() { { var f: () => void; } f(); }
f();