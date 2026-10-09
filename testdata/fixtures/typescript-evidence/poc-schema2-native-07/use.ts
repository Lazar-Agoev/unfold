try {} catch (error) { error(); }
const holder = () => { try {} catch ({ callback }) { callback(); } };
let second = () => { try {} catch ([callback]) { callback(); } };
let mutable; mutable(); var variable; variable();