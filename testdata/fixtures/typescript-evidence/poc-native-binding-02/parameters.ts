export {};
function f() {}
function parameter(f: () => void) { f(); }
function destructured({ f }: { f: () => void }) { f(); }
try {} catch (f) { f(); }
try {} catch ({ f }) { f(); }
f();