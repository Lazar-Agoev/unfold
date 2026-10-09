// Statements: every loop, exception, switch, label, transfer, directive, and automatic semicolon insertion; an explicit this parameter takes a parameter slot.
async function run(this: Window, a = first()) {
    "use strict";
    outer: for (let index = initial(); condition(); update()) { if (stop()) break outer; continue; }
    for (var key in object()) use(key);
    for await (const item of items()) use(item);
    for (target of targets) handle();
    while (condition()) body();
    do body(); while (condition());
    switch (input()) { case match(): use(); break; default: fallback(); }
    switch (empty) {}
    try { throw failure(); } catch (error: unknown) { recover(error); } finally { cleanup(); }
    try { action(); } catch { recover(); }
    if (a) b(); else if (c) d(); else e();
    if (x);
    return
    finish()
}
