export {};
class A { m() {} run() { (this as B).m(); (<B>this).m(); this.m(); } }
class B { m() {} }
class Child extends B { m() {} }