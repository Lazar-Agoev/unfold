// Type syntax, ambient declarations, and overload signatures get no blocks; interfaces, type aliases, enums, and calls beneath enums and namespaces do.
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
