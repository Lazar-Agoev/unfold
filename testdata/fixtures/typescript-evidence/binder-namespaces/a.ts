export {};
namespace A { export function f() {} function hidden() {} hidden(); }
namespace A { f(); hidden(); export const g = () => f(); }
A.f(); A.g();
namespace B.C { export function deep() {} }
namespace B { C.deep(); }
B.C.deep();
enum E { One = 1, Two = One + 1 }
enum E { Three = Two + 1 }
function merged() {}
namespace merged { export function helper() {} }
merged(); merged.helper();
