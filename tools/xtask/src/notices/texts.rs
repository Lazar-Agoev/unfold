//! Fixed texts for the notices: the MIT license body for packages that ship no license file, the
//! copyright lines of the ones Unfold names explicitly, and the components that come from neither
//! Cargo nor the npm production tree (Electron and Chromium, `@types/node`).

/// The MIT license after its copyright line.
pub(super) const MIT_BODY: &str = "\
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.";

/// Copyright lines, from the upstream repositories' `LICENSE` files, of MIT crates whose published
/// packages carry no license file. Matched by crate-name prefix.
pub(super) const MIT_COPYRIGHTS: &[(&str, &str)] = &[
    (
        "oxc",
        "Copyright (c) 2024-present VoidZero Inc. & Contributors\nCopyright (c) 2023 Boshen",
    ),
    ("napi", "Copyright (c) 2020 LongYinan"),
];

/// The explicit notices, in Markdown, before the generated lists. `{typescript_license}` is
/// replaced by TypeScript's own `LICENSE.txt`.
pub(super) const EXPLICIT: &str = r#"## Notices for the main components

**Electron and Chromium.** Unfold runs on Electron (MIT License, Copyright (c) Electron
contributors, Copyright (c) 2013-2020 GitHub Inc.). Electron embeds Chromium, Node.js, V8, and
their own dependencies, under BSD-style and other licenses listed in `LICENSES.chromium.html`. That
file and Electron's `LICENSE.electron.txt` ship with the app: beside the executable on Windows and
Linux, and in `Unfold.app/Contents/Resources` on macOS.

```text
Copyright (c) Electron contributors
Copyright (c) 2013-2020 GitHub Inc.

{mit_body}
```

**oxc** (MIT) parses TypeScript: the `oxc_*` crates, listed under Rust crates below with their
license text.

**Ruff** (MIT, Copyright (c) 2022 Charles Marsh) parses Python: `ruff_python_parser`,
`ruff_python_ast`, `ruff_python_trivia`, `ruff_source_file`, and `ruff_text_size`, pinned to a Ruff
release tag. Ruff's license file, reproduced below, also carries the notices of the projects Ruff's
code derives from, among them RustPython (MIT).

**napi-rs** (MIT, Copyright (c) 2020 LongYinan) binds the Rust engine to Node-API: the `napi`,
`napi-sys`, `napi-derive`, and `napi-build` crates.

**TypeScript's `lib` files.** Unfold's tables of JavaScript globals, library types, and Node.js
built-in modules are generated from TypeScript's `lib.*.d.ts` files (Apache License 2.0, Copyright
(c) Microsoft Corporation) and from `@types/node` (MIT License, Copyright (c) Microsoft Corporation,
from DefinitelyTyped). The tables hold names extracted from those files, not the files themselves.

```text
{typescript_license}
```

```text
Copyright (c) Microsoft Corporation.

{mit_body}
```
"#;
