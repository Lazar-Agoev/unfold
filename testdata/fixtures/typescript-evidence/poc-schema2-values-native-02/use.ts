class Base {
    method() {} static staticMethod() {} arrow = () => {};
    run() { const refs = [this.method, this.arrow, Base.staticMethod]; const nested = () => { const refs = [this.method]; }; function rebound() { const refs = [this.method]; } }
}
class Child extends Base { method() {} run() { const refs = [super.method, this.method]; } }