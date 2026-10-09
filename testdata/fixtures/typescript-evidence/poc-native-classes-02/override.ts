export {};
class Base { m() {} run() { this.m(); } }
class Mid extends Base { again() { this.m(); super.m(); } }
class Leaf extends Mid { m() {} runLeaf() { this.m(); super.m(); } }