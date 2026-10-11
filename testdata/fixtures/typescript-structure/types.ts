// Interfaces, type aliases, and enums are blocks unless ambient; calls in enum initializers are under the enum.
export interface Shape<T> extends Base, Other<T> { kind: string }
export default interface Fallback { x: number }
declare interface Ambient { y: number }
interface Merged { a: number }
interface Merged { b: number }
export type Pair<A, B> = [A, B];
type Literal = { a: string; b?: number };
export const enum Mode { On = 1 }
declare enum AmbientEnum { A }
enum Status { Ready = start(), Done = finish("x") }
namespace Shapes { export interface Circle { r: number } export enum Unit { Px = scale() } type Local = string; }
declare namespace Lib { interface Hidden {} type T = 1; enum E { A } }
declare global { interface Window { app: unknown } }
declare module "pkg" { interface Exported {} }
function scoped() { interface Inner { z: number } enum Local { A = compute() } }
