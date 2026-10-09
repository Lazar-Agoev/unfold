export {};
class Repo { save() {} }
function inferred() { return new Repo(); }
inferred().save();
const values = [1, 2]; values.map(value => value);