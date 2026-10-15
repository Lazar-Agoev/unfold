//! Fixed texts for the notices: the MIT license body for packages that ship no license file, the
//! copyright lines of the ones Unfold names explicitly, and the components that come from neither
//! Cargo nor the npm production tree (Electron and Chromium, `@types/node`, typeshed, and the code
//! the MCP SDK's published files carry: check its versions when the SDK is upgraded).

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

/// typeshed's `LICENSE` at the commit `cargo xtask gen-py-stubs` reads (the Python front end's
/// standard-library tables come from its stubs).
pub(super) const TYPESHED_LICENSE: &str = include_str!("typeshed-LICENSE.txt");

/// The explicit notices, in Markdown, before the generated lists. `{typescript_license}` is
/// replaced by TypeScript's own `LICENSE.txt`, and `{typeshed_license}` by typeshed's `LICENSE`.
pub(super) const EXPLICIT: &str = r#"## Notices for the main components

**Electron and Chromium.** Unfold runs on Electron (MIT License, Copyright (c) Electron
contributors, Copyright (c) 2013-2020 GitHub Inc.). Electron embeds Chromium, Node.js, V8, and
their own dependencies, under BSD-style and other licenses listed in `LICENSES.chromium.html`. That
file and Electron's `LICENSE.electron.txt` ship with the app, in `Unfold.app/Contents/Resources`.

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

**The MCP TypeScript SDK** (`@modelcontextprotocol/server`, listed under npm packages below) serves
the app's local MCP server for agents. Its published files carry code of its own dependencies, which
the app ships with it: Ajv 8.18.0, ajv-formats 3.0.1, fast-deep-equal 3.1.3, and
json-schema-traverse 1.0.0 (MIT, Copyright (c) Evgeny Poberezkin), content-type 1.0.5 (MIT,
Copyright (c) 2015 Douglas Christopher Wilson), and fast-uri 3.1.0 (BSD-3-Clause).

```text
Copyright (c) 2015-2021 Evgeny Poberezkin (Ajv)
Copyright (c) 2020 Evgeny Poberezkin (ajv-formats)
Copyright (c) 2017 Evgeny Poberezkin (fast-deep-equal, json-schema-traverse)
Copyright (c) 2015 Douglas Christopher Wilson (content-type)

{mit_body}
```

```text
Copyright (c) 2011-2021, Gary Court until https://github.com/garycourt/uri-js/commit/a1acf730b4bba3f1097c9f52e7d9d3aba8cdcaae
Copyright (c) 2021-present The Fastify team
All rights reserved.

The Fastify team members are listed at https://github.com/fastify/fastify#team.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:
    * Redistributions of source code must retain the above copyright
      notice, this list of conditions and the following disclaimer.
    * Redistributions in binary form must reproduce the above copyright
      notice, this list of conditions and the following disclaimer in the
      documentation and/or other materials provided with the distribution.
    * The names of any contributors may not be used to endorse or promote
      products derived from this software without specific prior written
      permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDERS AND CONTRIBUTORS BE LIABLE FOR ANY
DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
(INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND
ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

                                  *   *   *

The complete list of contributors can be found at:
- https://github.com/garycourt/uri-js/graphs/contributors
```

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

**typeshed's standard-library stubs.** Unfold's tables of the Python standard library (its module
names, and the functions, classes, and written types of eight modules) are generated from
typeshed's `stdlib` stubs at commit `a205439338a4ad3debec1eeae7d300e3781c066d`, the copy bundled
with Pyright 1.1.407 (Apache License 2.0, with parts under the MIT License, Copyright (c) 2015
Jukka Lehtosalo and contributors). The tables hold names and types extracted from those files, not
the files themselves.

```text
{typeshed_license}
```
"#;
