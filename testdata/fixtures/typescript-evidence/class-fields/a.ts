export class Repo {
  save(): void {}
}
export class Archive {
  save(): void {}
}

export class Kept {
  repo = new Repo();
  readonly archive = new Archive();
  run(): void {
    this.repo.save();
    this.archive.save();
  }
}

export class Swapped {
  repo = new Repo();
  run(): void {
    this.repo.save();
  }
  swap(): void {
    this.repo = new Archive();
  }
}

export class Constructed {
  readonly repo = new Repo();
  constructor(fresh: boolean) {
    if (fresh) this.repo = new Archive();
  }
  run(): void {
    this.repo.save();
  }
}

export class Exposed {
  repo = new Repo();
  run(): void {
    this.repo.save();
  }
}

export function expose(exposed: Exposed): void {
  exposed.repo = new Archive();
}

export class Hidden {
  #repo = new Repo();
  static shared = new Repo();
  run(): void {
    this.#repo.save();
    Hidden.shared.save();
  }
  swap(): void {
    this.#repo = new Archive();
  }
}

export function share(): void {
  Hidden.shared = new Archive();
}

export class Base {
  repo = new Repo();
  run(): void {
    this.repo.save();
  }
}

export class Derived extends Base {
  repo = new Archive();
}

export class Annotated {
  repo: Repo = new Repo();
  run(): void {
    this.repo.save();
  }
  swap(): void {
    this.repo = new Archive();
  }
}

export function helper(): void {}
export function other(): void {}

export class Handlers {
  handler = (): void => helper();
  rebound = (): void => helper();
  run(): void {
    this.handler();
    this.rebound();
  }
  swap(): void {
    this.rebound = other;
  }
}
