export {};
class Repo { save() {} }
function run(values: Repo[], index: number) { values[0].save(); values[index].save(); values["0"].save(); values.map(value => value); values["map"](value => value); }