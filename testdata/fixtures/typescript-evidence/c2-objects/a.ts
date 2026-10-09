export const service = {
  start() { return 1; },
  stop: function () {},
  arrow: () => 2,
  value: 3,
  get size() { return 0; },
};
service.start(); service.stop(); service.arrow(); service.value; service.size;
const written = { run() {} };
written.run = () => {};
written.run();
const spread = { ...service, extra() {} };
spread.extra();
interface Shape { draw(): void }
const annotated: Shape = { draw() {} };
annotated.draw();
const asserted = { draw() {} } as Shape;
asserted.draw();
let mutable = { go() {} };
mutable.go();
const computed = { ["lit"]() {}, [Symbol.iterator]() {} };
computed.lit();
