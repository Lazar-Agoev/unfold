export {};
class Repo { save() {} }
class Box { constructor(public repo: Repo) {} }
class Holder { constructor(private box: Box) {} run() { this.box.repo.save(); } }
function call(box: Box) { box.repo.save(); }