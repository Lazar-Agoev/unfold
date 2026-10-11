// TypeScript rejects these (a type can't be its own base), but they are written, so they are
// linked; a definition lists each base once.
export interface Loop extends Knot {
  loop: string;
}

export interface Knot extends Loop {
  knot: string;
}
