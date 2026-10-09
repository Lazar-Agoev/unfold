export {};
function f() {}
for (const f = () => {}; flag;) { f(); }
for (const f of values) { f(); }
for (const f in values) { f(); }
switch (flag) { case 0: const f = () => {}; f(); break; case 1: f(); break; }
f();