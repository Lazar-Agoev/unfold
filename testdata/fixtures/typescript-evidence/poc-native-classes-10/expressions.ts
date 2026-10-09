export {};
const C = class Self { constructor() {} static make() { return new Self(); } m() {} run() { this.m(); } };
new C(); C.make();