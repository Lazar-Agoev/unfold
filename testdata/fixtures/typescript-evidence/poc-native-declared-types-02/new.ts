export {};
class Repo { save() {} }
const repo = new Repo(); repo.save();
class Holder { repo = new Repo(); run() { this.repo.save(); } }
new Repo().save();