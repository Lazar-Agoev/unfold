class Leaf {
  save(): number {
    return 1;
  }
}

class Other {
  save(): number {
    return 2;
  }
}

function getLeaf(): Leaf {
  return new Leaf();
}

async function loadLeaf(): Promise<Leaf> {
  return new Leaf();
}

export async function assigned(flag: boolean, list: Other[], pair: { m: Leaf }): Promise<void> {
  let viaNew = new Leaf();
  viaNew = new Other();
  viaNew.save();

  let viaCall = getLeaf();
  if (flag) {
    viaCall = new Other();
  }
  viaCall.save();

  let viaAwait = await loadLeaf();
  viaAwait = new Other();
  viaAwait.save();

  let inClosure = getLeaf();
  const swap = () => {
    inClosure = new Other();
  };
  swap();
  inClosure.save();

  let destructured = getLeaf();
  [destructured] = list;
  destructured.save();

  let shorthand = getLeaf();
  ({ m: shorthand } = pair);
  shorthand.save();

  let looped = getLeaf();
  for (looped of list) {
    looped.save();
  }

  var hoisted = getLeaf();
  hoisted = new Other();
  hoisted.save();
}

export async function held(): Promise<void> {
  const constant = getLeaf();
  constant.save();
  let once = getLeaf();
  once.save();
  var single = new Leaf();
  single.save();
  let awaited = await loadLeaf();
  awaited.save();
  let annotated: Leaf = getLeaf();
  annotated = new Other();
  annotated.save();
}

export function defaulted(leaf = new Leaf()): void {
  leaf.save();
}

export function use(): void {
  defaulted(new Other());
}
