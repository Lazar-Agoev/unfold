export {};
class Repo { save() {} }
function make(): Repo { return new Repo(); }
class Factory { make(): Repo { return new Repo(); } run() { this.make().save(); } }
make().save();