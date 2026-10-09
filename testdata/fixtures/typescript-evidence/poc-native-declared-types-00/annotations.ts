export {};
class Repo { save() {} }
function call(repo: Repo) { repo.save(); }
let repo: Repo; repo.save();
class Holder { repo!: Repo; run() { this.repo.save(); } }