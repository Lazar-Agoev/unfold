export {};
function local(process: NodeJS.Process) { process.cwd(); }
process.cwd();
function shadowed(console: Console) { console.log("x"); }
