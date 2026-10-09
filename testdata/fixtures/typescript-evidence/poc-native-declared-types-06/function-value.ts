export {};
class Repo { save() {} }
function factory(): Repo { return new Repo(); }
factory.save();