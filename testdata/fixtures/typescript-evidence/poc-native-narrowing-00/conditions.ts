export {};
function check(value: C): boolean { return true; }
function host(value: C): object | undefined { return {}; }
class C { m() {} run() { if (check(this)) this.m(); host(this) || this.m(); } }