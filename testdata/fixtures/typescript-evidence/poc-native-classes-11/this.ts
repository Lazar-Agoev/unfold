export {};
class C {
    m() {}
    run() {
        const arrow = () => () => this.m();
        function nested(this: C) { this.m(); const arrow = () => this.m(); }
        const expression = function (this: C) { this.m(); };
    }
}