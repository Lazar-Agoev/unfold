export {};
class Repo { save() {} }
class Other { save() {} }
class Base { #repo!: Repo; run(value: Child) { value.#repo.save(); } }
class Child extends Base { #repo!: Other; }