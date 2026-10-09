export {};
function decorate(value: unknown) { return () => {}; }
class Outer {
    key() { return "name"; }
    run() {
        class Inner {
            [this.key()]() {}
            @decorate(this.key()) method() {}
            field = this.key();
        }
    }
}