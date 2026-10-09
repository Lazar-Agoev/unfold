export {};
function F() {} namespace F { export function extra() {} }
class C {} namespace C { export function extra() {} }
enum E { A } namespace E { export function extra() {} }
F(); F.extra(); C.extra(); E.extra();