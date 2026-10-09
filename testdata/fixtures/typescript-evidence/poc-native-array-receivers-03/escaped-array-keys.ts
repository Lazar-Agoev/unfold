export {}; class Repo { save() {} }
function run(values: Repo[]) { values[`\x30`].save(); values["\x30"].save(); values[`\u0030`].save(); }