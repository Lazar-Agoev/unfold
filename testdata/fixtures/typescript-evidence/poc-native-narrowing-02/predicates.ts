export {};
class C { m() {} }
class D { m() {} }
function isD(value: C): value is D { return true; }
class Holder { value!: C; m() {} run() { if (isD(this.value)) this.value.m(); this.m(); } }