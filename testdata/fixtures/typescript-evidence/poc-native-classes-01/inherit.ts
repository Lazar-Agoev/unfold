export {};
class Base { m() {} }
class Mid extends Base { run() { this.m(); } }
class Leaf extends Mid { runAgain() { this.m(); super.m(); } }