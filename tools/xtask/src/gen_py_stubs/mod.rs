//! `cargo xtask gen-py-stubs <typeshed-dir> [--check]`: the Python front end's table of eight
//! standard modules (`builtins`, `logging`, `os`, `json`, `re`, `datetime`, `pathlib`, `asyncio`),
//! generated from typeshed's `.pyi` stubs and read without generics.
//!
//! The stubs are read at the typeshed commit pinned in `crates/unfold-lang-python/src/stdlib.rs`
//! (the copy bundled with Pyright 1.1.407), from a checkout of typeshed or Pyright's
//! `typeshed-fallback` folder, parsed with Ruff ([`stubs`]), never imported or run. The files
//! read must hash to [`STUB_SET_SHA256`], so no other typeshed is written under the pinned
//! commit. The table ([`table`]) holds each module's members, with the written types of its
//! functions and variables, and every class those reach through members, return types,
//! attributes, and bases, wherever typeshed defines it (`asyncio.Task` is `_asyncio.Task`). It is
//! read once for each Python version a project may declare and intersected, so it holds only
//! what every version has. It is written to `crates/unfold-lang-python/src/stubs.rs` as sorted
//! static arrays, with the commit, the Python versions, and the stub set's SHA-256 in its header.
//! The builtin exception classes and their bases are also written, for the reader's catches, to
//! `crates/unfold-reader/src/contracts/python_exceptions.rs` ([`render_exceptions`]). With
//! `--check`, the files are compared instead of written.

mod stubs;
mod table;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use sha2::{Digest, Sha256};
use unfold_engine::settings::PythonVersion;

use crate::repo;
use stubs::Loader;
use table::{Member, Returns, Table, TableClass};

/// The typeshed commit the table is read from, as pinned in `unfold-lang-python`'s `stdlib.rs`.
const TYPESHED_COMMIT: &str = "a205439338a4ad3debec1eeae7d300e3781c066d";

/// The SHA-256 of the stub files the table is read from at [`TYPESHED_COMMIT`] ([`digest`]).
/// Reading other files (another typeshed, or a generator that follows more modules) fails until
/// this is updated from a checkout of the pinned commit.
const STUB_SET_SHA256: &str = "da36d0a6d4ccca027f9306f358ca33cc6f7c67c98e1533b683c3bcf89794716d";

/// The modules the table covers. A submodule a covered module binds (`os.path`) is covered too.
const MODULES: &[&str] = &[
    "asyncio", "builtins", "datetime", "json", "logging", "os", "pathlib", "re",
];

/// Arguments of `gen-py-stubs`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// A typeshed checkout at the pinned commit (or Pyright's `typeshed-fallback` folder): the
    /// folder holding `stdlib/`.
    typeshed: PathBuf,
    /// Compare the generated file with the checked-in one instead of writing it.
    #[arg(long)]
    check: bool,
}

/// Where the table is checked in.
fn output_path() -> PathBuf {
    repo::root().join("crates/unfold-lang-python/src/stubs.rs")
}

/// Where the reader's table of builtin exception classes is checked in.
fn exceptions_path() -> PathBuf {
    repo::root().join("crates/unfold-reader/src/contracts/python_exceptions.rs")
}

/// Generates the tables and writes or checks them.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    let stdlib = args.typeshed.join("stdlib");
    if !stdlib.join("VERSIONS").is_file() {
        bail!(
            "{} is not a typeshed folder (no stdlib/VERSIONS)",
            args.typeshed.display()
        );
    }
    let generated = generate(&stdlib)?;
    for (path, text) in [
        (output_path(), &generated.stubs),
        (exceptions_path(), &generated.exceptions),
    ] {
        repo::check_or_write(&path, text, args.check, "cargo xtask gen-py-stubs <typeshed-dir>")?;
    }
    Ok(())
}

/// The generated Rust files: the front end's table and the reader's builtin exceptions.
#[derive(Debug)]
struct Generated {
    stubs: String,
    exceptions: String,
}

/// The generated Rust files for the `stdlib/` folder.
fn generate(stdlib: &Path) -> anyhow::Result<Generated> {
    let mut tables = Vec::new();
    let mut files = BTreeMap::new();
    for version in PythonVersion::ALL {
        let mut loader = Loader::new(stdlib.to_owned(), parse_version(version.as_str())?);
        tables.push(table::build(&mut loader, MODULES)?);
        for file in loader.files {
            files.entry(file.name).or_insert(file.text);
        }
    }
    let digest = digest(&files);
    if digest != STUB_SET_SHA256 {
        bail!(
            "the stub files read hash to {digest}, not {STUB_SET_SHA256}: {} is not typeshed {TYPESHED_COMMIT}",
            stdlib.display()
        );
    }
    let (first, last) = match PythonVersion::ALL {
        [first, .., last] => (first.as_str(), last.as_str()),
        _ => bail!("the table needs at least two Python versions"),
    };
    let header = Header {
        commit: TYPESHED_COMMIT,
        python_versions: &format!("{first}–{last}"),
        files: files.len(),
        digest: &digest,
    };
    let table = table::intersect(tables)?;
    Ok(Generated {
        stubs: render(&header, &table),
        exceptions: render_exceptions(&header, &table),
    })
}

/// `3.14` as `(3, 14)`.
fn parse_version(version: &str) -> anyhow::Result<(u64, u64)> {
    let (major, minor) = version
        .split_once('.')
        .with_context(|| format!("`{version}` is not a major.minor version"))?;
    Ok((major.parse()?, minor.parse()?))
}

/// SHA-256 over the files (by name, sorted): each name, NUL, its text, NUL.
fn digest(files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (name, text) in files {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(text.as_bytes());
        hasher.update([0]);
    }
    hasher.finalize().iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// What the generated file's header says.
struct Header<'a> {
    commit: &'a str,
    /// The Python versions the table holds for (`3.12–3.14`).
    python_versions: &'a str,
    files: usize,
    digest: &'a str,
}

/// The widest generated line.
const WIDTH: usize = 110;

/// The hand-written part of the generated file: the shapes of the table.
const TYPES: &str = r#"
/// What calling a stub function or method gives, from its written return type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Returns {
    /// No written return type in the accepted forms (or `None`).
    Unknown,
    /// An instance of the class, by its qualified name (`logging.Logger`).
    Instance(&'static str),
    /// An instance of the receiver's class (`Self`): the class the method is looked up on.
    Receiver,
    /// The value of awaiting the call (an `async def`): an instance of the class.
    AwaitInstance(&'static str),
    /// The value of awaiting the call: an instance of the receiver's class.
    AwaitReceiver,
}

/// One member of a stub module or class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Member {
    /// A function or method.
    Function(Returns),
    /// A class, by its qualified name (an entry of [`CLASSES`]).
    Class(&'static str),
    /// A submodule (an entry of [`MODULES`]).
    Module(&'static str),
    /// A variable, attribute, or property, with its written class when it has one.
    Variable(Option<&'static str>),
    /// A property written `Self`: the instance it is read on.
    SelfVariable,
}

/// A covered standard module and its members, sorted by name.
#[derive(Debug)]
pub(crate) struct StubModule {
    pub(crate) name: &'static str,
    pub(crate) members: &'static [(&'static str, Member)],
}

/// A class: its qualified name (where typeshed defines it: `_asyncio.Task`), its base classes in
/// written order (`Generic[…]` and `Protocol[…]` left out; `object` only where written), whether
/// it is a protocol (`Protocol` among its written bases), and the members its own body declares,
/// sorted by name.
#[derive(Debug)]
pub(crate) struct StubClass {
    pub(crate) name: &'static str,
    pub(crate) bases: &'static [&'static str],
    pub(crate) protocol: bool,
    pub(crate) members: &'static [(&'static str, Member)],
}
"#;

/// The generated file's first lines: what generated it, and from which stubs.
fn source_lines(out: &mut String, header: &Header<'_>) {
    out.push_str("//! Generated by `cargo xtask gen-py-stubs <typeshed-dir>`; do not edit by hand.\n");
    let _ = writeln!(
        out,
        "//! Source: typeshed {} (`stdlib/`, {} files),\n//! read without generics for each of Python {}.",
        header.commit, header.files, header.python_versions
    );
    let _ = writeln!(out, "//! Stub-set SHA-256: {}.", header.digest);
}

fn render(header: &Header<'_>, table: &Table) -> String {
    let mut out = String::new();
    source_lines(&mut out, header);
    out.push_str(
        "//!\n\
         //! Eight standard modules and every class their members reach, in every version. A written\n\
         //! type is kept when it is a class (type arguments dropped), a class or `None`, or `Self`, and\n\
         //! every overload, platform branch, and version agrees on it; otherwise it is unknown.\n",
    );
    out.push_str(TYPES);
    out.push('\n');
    let used = used_variants(table);
    let members: Vec<&str> = ["Class", "Function", "Module", "SelfVariable", "Variable"]
        .into_iter()
        .filter(|variant| used.contains(variant))
        .collect();
    let returns: Vec<&str> = [
        "AwaitInstance",
        "AwaitReceiver",
        "Instance",
        "Receiver",
        "Unknown",
    ]
    .into_iter()
    .filter(|variant| used.contains(variant))
    .collect();
    for (owner, variants) in [("Member", members), ("Returns", returns)] {
        match variants[..] {
            [] => {}
            [variant] => {
                let _ = writeln!(out, "use {owner}::{variant};");
            }
            _ => {
                let _ = writeln!(out, "use {owner}::{{{}}};", variants.join(", "));
            }
        }
    }

    out.push_str("\n/// The covered modules, sorted by name.\n#[rustfmt::skip]\n");
    out.push_str("pub(crate) static MODULES: &[StubModule] = &[\n");
    for module in &table.modules {
        let _ = writeln!(out, "    StubModule {{ name: {:?}, members: &[", module.name);
        member_rows(&mut out, &module.members);
        out.push_str("    ] },\n");
    }
    out.push_str("];\n");

    out.push_str(
        "\n/// Every class the covered modules reach, sorted by qualified name.\n#[rustfmt::skip]\n",
    );
    out.push_str("pub(crate) static CLASSES: &[StubClass] = &[\n");
    for class in &table.classes {
        let bases: Vec<String> = class.bases.iter().map(|base| format!("{base:?}")).collect();
        let _ = writeln!(
            out,
            "    StubClass {{ name: {:?}, bases: &[{}], protocol: {}, members: &[",
            class.name,
            bases.join(", "),
            class.protocol
        );
        member_rows(&mut out, &class.members);
        out.push_str("    ] },\n");
    }
    out.push_str("];\n");
    out
}

/// The module whose exception classes the reader's table holds, and the root of those classes.
const BUILTINS: &str = "builtins";
const BASE_EXCEPTION: &str = "builtins.BaseException";

/// Deepest chain of bases followed to `BaseException`.
const MAX_BASES: usize = 32;

/// The reader's table of builtin exception classes: each name `builtins` binds to one, with the
/// class it names and that class's bases, all without their `builtins.` prefix. A class is a
/// builtin exception class when it is `BaseException`, or a `builtins` class whose every base is
/// one.
fn render_exceptions(header: &Header<'_>, table: &Table) -> String {
    let classes: BTreeMap<&str, &TableClass> = table
        .classes
        .iter()
        .map(|class| (class.name.as_str(), class))
        .collect();
    let mut out = String::new();
    source_lines(&mut out, header);
    out.push_str(
        "//!\n\
         //! Python's builtin exception classes, for telling which `except` catches a raise: each name\n\
         //! `builtins` binds to one, with the class it names (`IOError` names `OSError`) and that class's\n\
         //! bases in written order.\n\n",
    );
    out.push_str("/// Each builtin exception name, sorted: the class it names and that class's bases.\n");
    out.push_str("#[rustfmt::skip]\n");
    out.push_str("pub(crate) static BUILTIN_EXCEPTIONS: &[(&str, &str, &[&str])] = &[\n");
    let short = |name: &str| format!("{:?}", name.strip_prefix("builtins.").unwrap_or(name));
    let builtins = table.modules.iter().find(|module| module.name == BUILTINS);
    for (name, member) in builtins.into_iter().flat_map(|module| &module.members) {
        let Member::Class(class) = member else {
            continue;
        };
        let Some(found) = classes
            .get(class.as_str())
            .filter(|_| is_exception(class, &classes, 0))
        else {
            continue;
        };
        let bases: Vec<String> = found.bases.iter().map(|base| short(base)).collect();
        let _ = writeln!(out, "    ({name:?}, {}, &[{}]),", short(class), bases.join(", "));
    }
    out.push_str("];\n");
    out
}

/// Whether class `name` is a builtin exception class, `depth` bases below the one asked about.
fn is_exception(name: &str, classes: &BTreeMap<&str, &TableClass>, depth: usize) -> bool {
    if name == BASE_EXCEPTION {
        return true;
    }
    depth < MAX_BASES
        && name.starts_with("builtins.")
        && classes.get(name).is_some_and(|class| {
            !class.bases.is_empty()
                && class
                    .bases
                    .iter()
                    .all(|base| is_exception(base, classes, depth + 1))
        })
}

/// The members, as many to a line as fit.
fn member_rows(out: &mut String, members: &BTreeMap<String, Member>) {
    let indent = "       ";
    let mut line = String::from(indent);
    for (name, member) in members {
        let item = format!("({name:?}, {}),", member_text(member));
        if line.len() + 1 + item.len() > WIDTH && line.len() > indent.len() {
            out.push_str(&line);
            out.push('\n');
            line = String::from(indent);
        }
        line.push(' ');
        line.push_str(&item);
    }
    if line.len() > indent.len() {
        out.push_str(&line);
        out.push('\n');
    }
}

fn member_text(member: &Member) -> String {
    match member {
        Member::Function(returns) => format!("Function({})", returns_text(returns)),
        Member::Class(class) => format!("Class({class:?})"),
        Member::Module(module) => format!("Module({module:?})"),
        Member::Variable(Some(class)) => format!("Variable(Some({class:?}))"),
        Member::Variable(None) => "Variable(None)".to_owned(),
        Member::SelfVariable => "SelfVariable".to_owned(),
    }
}

fn returns_text(returns: &Returns) -> String {
    match returns {
        Returns::Unknown => "Unknown".to_owned(),
        Returns::Instance(class) => format!("Instance({class:?})"),
        Returns::Receiver => "Receiver".to_owned(),
        Returns::AwaitInstance(class) => format!("AwaitInstance({class:?})"),
        Returns::AwaitReceiver => "AwaitReceiver".to_owned(),
    }
}

/// The names of the enum variants the table uses, for its `use` lines.
fn used_variants(table: &Table) -> Vec<&'static str> {
    let mut used = Vec::new();
    let members = table
        .modules
        .iter()
        .flat_map(|module| module.members.values())
        .chain(table.classes.iter().flat_map(|class| class.members.values()));
    for member in members {
        let (kind, returns) = match member {
            Member::Function(returns) => ("Function", Some(returns)),
            Member::Class(_) => ("Class", None),
            Member::Module(_) => ("Module", None),
            Member::Variable(_) => ("Variable", None),
            Member::SelfVariable => ("SelfVariable", None),
        };
        used.push(kind);
        if let Some(returns) = returns {
            used.push(match returns {
                Returns::Unknown => "Unknown",
                Returns::Instance(_) => "Instance",
                Returns::Receiver => "Receiver",
                Returns::AwaitInstance(_) => "AwaitInstance",
                Returns::AwaitReceiver => "AwaitReceiver",
            });
        }
    }
    used.sort_unstable();
    used.dedup();
    used
}

#[cfg(test)]
mod tests;
