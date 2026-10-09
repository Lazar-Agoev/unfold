export {};
var outer = () => 1;
function params(a = outer(), b = (() => inner())()) { var outer = () => 2; function inner() {} outer(); return a + b; }
{ function blockScoped() {} blockScoped(); }
blockScoped();
for (var hoisted = () => 0; ; ) { break; }
hoisted();
const Named = class Inner { static make() { return Inner.make(); } };
Named.make();
try { } catch (caught) { (caught as any)(); }
label: { const local = () => 0; local(); }
function shadow(shadow: () => void) { shadow(); }
