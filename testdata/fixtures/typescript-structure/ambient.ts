// Type syntax, ambient declarations, and overload signatures are no nodes; interfaces, type aliases, and enums are, and calls in enum initializers and namespaces are code of the enclosing body.
type Alias = { [typeKey()]: string };
interface Shape { method(): void; [key: string]: unknown }
export enum Status { Ready = initialize(), Next = 2 }
namespace Outer.Inner { export const value = create(); export function run() { return finish(); } }
declare namespace Ambient { const hidden: number; function signature(): void; class Hidden {} }
declare const declared: number;
declare function declaredFunction(): void;
function overloaded(value: string): string;
function overloaded(value: unknown) { return value; }
const checked = (factory() as Map<string, Array<Foo>>) satisfies object;
const asserted = (<Foo>make())!;
