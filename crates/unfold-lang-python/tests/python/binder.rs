//! The binder's rules, ported from the proof of concept's binder, import, and metaclass tests:
//! the write census, decorators by identity, receivers and C3, construction, `super()`,
//! reflection, imports and source roots, star imports, and named child imports.

use unfold_core::vocab::{CallRole, Form, LinkKind, Outcome, Reason};
use unfold_core::{Key, Link, LinkTarget};
use unfold_engine::settings::Settings;

use crate::common::{Compiled, compile_files, compile_files_with, testdata};

fn graph(sources: &[(&str, &str)]) -> Compiled {
    compile_files(sources, &Settings::default()).valid()
}

fn graph_with(sources: &[(&str, &str)], settings: &str) -> Compiled {
    compile_files(sources, &Settings::parse(settings).expect("valid settings")).valid()
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(testdata().join("fixtures/python/policy").join(name)).expect("read a fixture")
}

/// The `calls` links labeled `label`, in file then node order.
fn calls<'a>(compiled: &'a Compiled, label: &str) -> Vec<&'a Link> {
    compiled
        .graph
        .files
        .iter()
        .flat_map(|file| &file.links)
        .filter(|link| link.kind == LinkKind::Calls && &*link.text == label)
        .collect()
}

fn call<'a>(compiled: &'a Compiled, label: &str) -> &'a Link {
    calls(compiled, label)
        .first()
        .copied()
        .unwrap_or_else(|| panic!("no call `{label}`"))
}

/// The reason a link is `unknown`, if it is.
fn reason_of(link: &Link) -> Option<Reason> {
    match link.to {
        LinkTarget::Unknown(reason) => Some(reason),
        _ => None,
    }
}

/// The key a link reaches, if it reaches a project definition.
fn project(link: &Link) -> Option<&Key> {
    match &link.to {
        LinkTarget::Project(key) => Some(key),
        _ => None,
    }
}

/// The module label a link reaches, if it reaches a library or the builtins.
fn module(link: &Link) -> Option<String> {
    match &link.to {
        LinkTarget::Library(label) | LinkTarget::Builtin(label) => Some(label.as_str().to_owned()),
        _ => None,
    }
}

fn reason(compiled: &Compiled, label: &str) -> Option<Reason> {
    reason_of(call(compiled, label))
}

fn resolved(compiled: &Compiled, label: &str) -> bool {
    call(compiled, label).to.outcome() == Outcome::Project
}

/// The `imports` links of `file`: (text, outcome, reason, target path).
fn imports(compiled: &Compiled, file: &str) -> Vec<(String, Outcome, Option<Reason>, String)> {
    compiled
        .file(file)
        .links
        .iter()
        .filter(|link| link.kind == LinkKind::Imports)
        .map(|link| {
            let target = project(link)
                .map(|key| key.as_str().trim_end_matches('#').to_owned())
                .unwrap_or_default();
            (link.text.to_string(), link.to.outcome(), reason_of(link), target)
        })
        .collect()
}

fn import_texts(compiled: &Compiled, file: &str) -> Vec<String> {
    imports(compiled, file)
        .into_iter()
        .map(|(text, ..)| text)
        .collect()
}

#[test]
fn the_write_census_respects_scopes_and_every_kind_of_write() {
    let compiled = graph(&[("bindings.py", &fixture("bindings.py"))]);
    assert!(resolved(&compiled, "alias"));
    for label in [
        "reassigned",
        "globally_written",
        "deleted",
        "loop_target",
        "with_target",
        "except_target",
        "walrus_target",
        "callback",
    ] {
        assert_eq!(reason(&compiled, label), Some(Reason::MutableBinding), "{label}");
    }
    let stable = calls(&compiled, "stable");
    assert_eq!(stable[0].to.outcome(), Outcome::Project);
    assert_eq!(reason_of(stable[1]), Some(Reason::Parameter));
    assert_eq!(reason_of(stable[2]), Some(Reason::NotConcrete));
    assert_eq!(stable[3].to.outcome(), Outcome::Project);
}

#[test]
fn decorators_are_proven_by_identity() {
    let compiled = graph(&[("decorators.py", &fixture("decorators.py"))]);
    assert!(resolved(&compiled, "transparent"));
    assert_eq!(reason(&compiled, "wrapped"), Some(Reason::Wrapper));
    assert_eq!(reason(&compiled, "decoy"), Some(Reason::Wrapper));
    assert!(resolved(&compiled, "overloaded"));
    assert_eq!(reason(&compiled, "self.callback"), Some(Reason::Accessor));
    let forms: Vec<Form> = compiled
        .file("decorators.py")
        .nodes
        .iter()
        .map(|node| node.form)
        .collect();
    assert!(forms.contains(&Form::Getter) && forms.contains(&Form::Setter));
}

#[test]
fn receivers_constructors_and_c3_stay_conservative() {
    let compiled = graph(&[
        ("receivers.py", &fixture("receivers.py")),
        ("classes.py", &fixture("classes.py")),
    ]);
    let saves = calls(&compiled, "self.repo.save");
    assert_eq!(saves[0].to.outcome(), Outcome::Project);
    assert_eq!(saves[1].to.outcome(), Outcome::Unknown);
    let methods = calls(&compiled, "self.method");
    assert!(
        methods
            .iter()
            .any(|link| reason_of(link) == Some(Reason::Overridden))
    );
    assert!(
        methods
            .iter()
            .any(|link| reason_of(link) == Some(Reason::NotConcrete))
    );
    assert!(resolved(&compiled, "instance"));
    let plain = call(&compiled, "Plain");
    let construction = compiled
        .graph
        .files
        .iter()
        .find(|file| file.links.iter().any(|link| std::ptr::eq(link, plain)))
        .map(|file| file.nodes[plain.from as usize].form);
    assert_eq!(construction, Some(Form::New));
    assert!(
        project(plain).is_some_and(|key| key.as_str().ends_with("#Plain.__init__")),
        "{plain:?}"
    );
    assert_eq!(reason(&compiled, "Wrapped"), Some(Reason::Wrapper));
    assert_eq!(reason(&compiled, "Custom"), Some(Reason::Unsupported));
    assert_eq!(reason(&compiled, "UnknownMeta"), Some(Reason::Unsupported));
    assert!(
        calls(&compiled, "super().method")
            .iter()
            .any(|link| link.to.outcome() == Outcome::Unknown)
    );
    assert!(
        calls(&compiled, "self.repo.method")
            .iter()
            .all(|link| link.to.outcome() == Outcome::Unknown)
    );
}

#[test]
fn imports_resolve_packages_aliases_and_literal_stars() {
    let compiled = graph(&[
        ("src/pkg/__init__.py", "from .service import work as exposed\n"),
        (
            "src/pkg/service.py",
            "__all__ = ['work']\ndef work(): return 1\ndef hidden(): return 2\n",
        ),
        ("src/namespace/helper.py", "def help(): return 1\n"),
        (
            "src/main.py",
            "from pkg import exposed\nfrom pkg.service import *\nimport pkg.service\nfrom namespace.helper import help\nimport importlib\nexposed()\nwork()\npkg.service.work()\nhelp()\nloaded = importlib.import_module('pkg.service')\nloaded.work()\n",
        ),
    ]);
    for label in ["exposed", "work", "pkg.service.work", "help"] {
        assert!(resolved(&compiled, label), "{label}");
    }
    assert_eq!(call(&compiled, "loaded.work").to.outcome(), Outcome::Unknown);
    let to_service = import_texts(&compiled, "src/main.py")
        .into_iter()
        .filter(|text| text == "pkg.service")
        .count();
    assert_eq!(to_service, 3);
    let ambiguous = graph_with(
        &[
            ("left/service.py", "def work(): return 1\n"),
            ("right/service.py", "def work(): return 2\n"),
            ("main.py", "from service import work\nwork()\n"),
        ],
        "[[python.project]]\nroot = \".\"\nsource_roots = [\"left\", \"right\"]\n",
    );
    assert_eq!(reason(&ambiguous, "work"), Some(Reason::Ambiguous));
    assert_eq!(imports(&ambiguous, "main.py")[0].2, Some(Reason::Ambiguous));
}

#[test]
fn reflection_needs_builtin_identity_not_spelling() {
    let compiled = graph(&[
        ("evaluation.py", &fixture("evaluation.py")),
        ("bindings.py", &fixture("bindings.py")),
        ("dynamic.py", &fixture("dynamic.py")),
    ]);
    assert_eq!(reason(&compiled, "prefixed"), Some(Reason::Unsupported));
    assert_eq!(call(&compiled, "module.stable").to.outcome(), Outcome::Unknown);
    let targets: Vec<&Link> = compiled
        .file("evaluation.py")
        .links
        .iter()
        .filter(|link| &*link.text == "target")
        .collect();
    assert!(!targets.is_empty());
    assert!(
        targets
            .iter()
            .all(|link| reason_of(link) == Some(Reason::Unsupported)),
        "unbounded evaluation may change any global of its module"
    );
    let decoy = graph(&[(
        "decoy.py",
        "from functools import partial\ndef target(): return 1\ndef eval(value): return value\ndef getattr(value, name): return target\neval('plain text')\ngetattr(None, 'anything')\ntarget()\nprefixed = partial(target)\nprefixed()\n",
    )]);
    assert!(resolved(&decoy, "target"));
    assert!(
        calls(&decoy, "getattr")
            .iter()
            .all(|link| link.to.outcome() == Outcome::Project)
    );
    assert_eq!(reason(&decoy, "prefixed"), Some(Reason::Wrapper));
}

#[test]
fn unknown_stars_external_mutations_and_bad_relative_imports_withhold_targets() {
    let stars = graph(&[
        (
            "main.py",
            "def target(): return 1\nfrom source import *\ntarget()\n",
        ),
        ("source.py", "def target(): return 2\n__all__ = names()\n"),
    ]);
    assert_eq!(reason(&stars, "target"), Some(Reason::Ambiguous));
    for decorator in ["functools.cache", "cache"] {
        let source = format!(
            "import functools\nfrom functools import cache\ndef replacement(fn): return lambda: 1\nfunctools.cache = replacement\n@{decorator}\ndef target(): return 1\ntarget()\n"
        );
        let mutated = graph(&[("main.py", &source)]);
        assert_eq!(reason(&mutated, "target"), Some(Reason::Wrapper), "{decorator}");
    }
    let relative = graph(&[
        ("pkg/main.py", "from ...source import target\ntarget()\n"),
        ("main.py", "from .source import target\ntarget()\n"),
        ("source.py", "def target(): return 1\n"),
    ]);
    assert!(
        calls(&relative, "target")
            .iter()
            .all(|link| reason_of(link) == Some(Reason::NotFound))
    );
    for file in ["main.py", "pkg/main.py"] {
        assert!(
            imports(&relative, file)
                .iter()
                .all(|row| row.2 == Some(Reason::NotFound))
        );
    }
    let namespace = graph(&[
        ("src/pkg/main.py", "from .source import target\ntarget()\n"),
        ("src/pkg/source.py", "def target(): return 1\n"),
    ]);
    assert!(resolved(&namespace, "target"));
}

#[test]
fn construction_and_super_refuse_inherited_metaclasses_expanded_headers_and_arguments() {
    let compiled = graph(&[(
        "main.py",
        "class Meta(type):\n    def __call__(self): return 1\nclass Base(metaclass=Meta): pass\nclass Derived(Base): pass\nDerived()\nclass Expanded(**kwargs): pass\nExpanded()\nclass Root:\n    def work(self): return 1\nclass Middle(Root):\n    def work(self): return 2\nclass Child(Middle):\n    def run(self): return super(Middle, self).work()\n",
    )]);
    assert_eq!(reason(&compiled, "Derived"), Some(Reason::Unsupported));
    assert_eq!(reason(&compiled, "Expanded"), Some(Reason::Unsupported));
    assert_eq!(
        reason(&compiled, "super(Middle, self).work"),
        Some(Reason::Unsupported)
    );
    let uncertain = graph(&[(
        "main.py",
        "class Root:\n    def work(self): return 1\nclass Base(Root):\n    def run(self): return super().work()\nclass Child(unknown(), Base): pass\n",
    )]);
    assert_eq!(reason(&uncertain, "super().work"), Some(Reason::Unsupported));
}

#[test]
fn virtual_calls_need_one_implementation_across_every_receiver() {
    let prefix = "class Base:\n    def work(self): return 1\n    def run(self): return self.work()\n";
    for descendant in [
        "class Child(Base):\n    @staticmethod\n    def work(): return 2\n",
        "class Other:\n    def work(self): return 2\nclass Child(Other, Base): pass\n",
        "class Child(unknown(), Base): pass\n",
        "class Child(Base):\n    def __init__(self): self.work = unknown()\n",
        "class Child(Base):\n    def __getattribute__(self, name): return unknown\n",
    ] {
        let compiled = graph(&[("main.py", &format!("{prefix}{descendant}"))]);
        assert_eq!(
            reason(&compiled, "self.work"),
            Some(Reason::Overridden),
            "{descendant}"
        );
    }
    let reflective = graph(&[(
        "main.py",
        &prefix.replace(
            "class Base:",
            "class Base:\n    def __getattribute__(self, name): return replacement",
        ),
    )]);
    assert_eq!(reason(&reflective, "self.work"), Some(Reason::Dynamic));
    let field = graph(&[(
        "main.py",
        "class Repo:\n    def save(self): return 1\nclass Service:\n    repo: Repo\n    def __init__(self): self.repo = unknown()\n    def run(self): return self.repo.save()\n",
    )]);
    assert_eq!(call(&field, "self.repo.save").to.outcome(), Outcome::Unknown);
    let inherited = graph(&[(
        "main.py",
        "class Repo:\n    def save(self): return 1\nclass Service:\n    repo: Repo\n    def run(self): return self.repo.save()\nclass Changed(Service):\n    def __init__(self): self.repo = unknown()\n",
    )]);
    assert_eq!(reason(&inherited, "self.repo.save"), Some(Reason::Dynamic));
    let class_receiver = graph(&[(
        "main.py",
        "class Base:\n    @staticmethod\n    def work(): return 1\n    @classmethod\n    def run(cls): return cls.work()\nclass Child(Base):\n    @staticmethod\n    def work(): return 2\n",
    )]);
    assert_eq!(reason(&class_receiver, "cls.work"), Some(Reason::Overridden));
}

#[test]
fn type_parameter_scopes_and_type_aliases_shadow_callables() {
    let compiled = graph_with(
        &[(
            "main.py",
            "def T(): return 1\ndef outer[T](value: T()): return T()\nclass Generic[T]:\n    def run(self): return T()\ndef target(): return 1\ntype target = int\ntarget()\n",
        )],
        "[[python.project]]\nroot = \".\"\nversion = \"3.12\"\n",
    );
    let t = calls(&compiled, "T");
    assert_eq!(t.len(), 3);
    assert!(t.iter().all(|link| link.to.outcome() == Outcome::Unknown));
    assert_eq!(reason(&compiled, "target"), Some(Reason::MutableBinding));
}

#[test]
fn known_opaque_values_are_not_concrete() {
    let compiled = graph(&[
        (
            "main.py",
            "import importlib\nfrom abc import ABC, abstractmethod\nloaded = importlib.import_module('service')\nloaded.work()\nclass Example(ABC):\n    @abstractmethod\n    def send(self): pass\n    def run(self): return self.send()\nclass Factory:\n    @classmethod\n    def create(cls): return cls()\n",
        ),
        ("service.py", "def work(): return 1\n"),
    ]);
    for label in ["loaded.work", "self.send", "cls"] {
        assert_eq!(reason(&compiled, label), Some(Reason::NotConcrete), "{label}");
    }
    let unproven = graph(&[(
        "pkg/main.py",
        "import importlib\nimportlib.import_module(package='other', name='service')\nimportlib.import_module('.service', 'other')\nimportlib.import_module(**options)\n",
    )]);
    assert_eq!(
        calls(&unproven, "importlib.import_module").len(),
        3,
        "an unproven argument binding never imports"
    );
}

#[test]
fn local_shadows_of_standard_modules_are_local() {
    let compiled = graph(&[
        ("os.py", "def work(): return 1\n"),
        ("main.py", "import os\nos.work()\n"),
    ]);
    assert!(resolved(&compiled, "os.work"));
}

#[test]
fn external_labels_come_from_the_inventory_or_the_package_name() {
    let compiled = graph(&[(
        "main.py",
        "import json\nimport os.path\nfrom fastapi import FastAPI\njson.dumps(1)\nos.path.join('a')\nFastAPI()\nprint(1)\nunknown_name()\n",
    )]);
    let label_of = |label: &str| module(call(&compiled, label));
    assert_eq!(label_of("json.dumps").as_deref(), Some("python:json"));
    assert_eq!(label_of("os.path.join").as_deref(), Some("python:os"));
    assert_eq!(label_of("FastAPI").as_deref(), Some("fastapi"));
    assert_eq!(label_of("print").as_deref(), Some("python:builtins"));
    assert_eq!(reason(&compiled, "unknown_name"), Some(Reason::UnknownName));
    let labels: Vec<Option<String>> = compiled
        .file("main.py")
        .links
        .iter()
        .filter(|link| link.kind == LinkKind::Imports)
        .map(module)
        .collect();
    assert_eq!(
        labels,
        [
            Some("python:json".to_owned()),
            Some("python:os".to_owned()),
            Some("fastapi".to_owned())
        ]
    );
}

#[test]
fn nfkc_equivalent_names_bind_and_nodes_keep_their_spelling() {
    let compiled = graph(&[("arguments.py", &fixture("arguments.py"))]);
    let link = call(&compiled, "Kelvin");
    let target = project(link).expect("Kelvin resolves");
    assert_eq!(
        compiled.node(target.as_str()).name.as_deref(),
        Some("\u{212A}elvin")
    );
    // Written ordinals never bind arguments to parameters.
    assert!(
        calls(&compiled, "fn")
            .iter()
            .all(|link| reason_of(link) == Some(Reason::Parameter))
    );
}

#[test]
fn the_policy_fixtures_compile_with_their_source_roots() {
    let root = tempfile::tempdir().expect("a temporary folder");
    copy(&testdata().join("fixtures/python/policy"), root.path());
    std::fs::write(
        root.path().join("unfold.toml"),
        "[[python.project]]\nroot = \"single-root\"\nsource_roots = [\"src\"]\n[[python.project]]\nroot = \"duplicate-roots\"\nsource_roots = [\"left\", \"right\"]\n",
    )
    .expect("write settings");
    let compiled = crate::common::compile_folder_with_threads(root.path(), 2).valid();
    let failed: Vec<&str> = compiled.failures.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(failed, ["encoding/latin1_cookie.py"]);
    assert_eq!(compiled.graph.files.len(), 23);
    let links = |file: &str, label: &str| -> Vec<Link> {
        compiled
            .file(file)
            .links
            .iter()
            .filter(|link| &*link.text == label)
            .cloned()
            .collect()
    };
    assert_eq!(
        links("imports/literal_star.py", "public")[0].to.outcome(),
        Outcome::Project
    );
    assert_eq!(
        links("imports/literal_star.py", "private")[0].to,
        LinkTarget::Unknown(Reason::UnknownName)
    );
    assert_eq!(
        links("imports/unknown_star.py", "candidate")[0].to,
        LinkTarget::Unknown(Reason::Ambiguous)
    );
    let guards = import_texts(&compiled, "imports/guards.py");
    assert_eq!(
        guards.iter().filter(|text| *text == ".literal_exports").count(),
        1
    );
    assert_eq!(
        links("imports/guards.py", "runtime_candidate")[0].to.outcome(),
        Outcome::Project
    );
    let dynamic = import_texts(&compiled, "imports/dynamic_imports.py");
    assert_eq!(
        dynamic
            .iter()
            .filter(|text| *text == "imports.literal_exports")
            .count(),
        2
    );
    for label in ["importlib.import_module", "import_module"] {
        assert_eq!(
            links("imports/dynamic_imports.py", label)[0].to.outcome(),
            Outcome::Project,
            "{label}"
        );
    }
    assert_eq!(
        links("imports/dynamic_imports.py", "genuine_import_module")[0]
            .to
            .outcome(),
        Outcome::Library
    );
    assert_eq!(import_texts(&compiled, "imports/multiple.py").len(), 3);
    assert_eq!(
        links("single-root/consumer.py", "selected")[0].to.outcome(),
        Outcome::Project
    );
    assert_eq!(
        links("duplicate-roots/consumer.py", "selected")[0].to,
        LinkTarget::Unknown(Reason::Ambiguous)
    );
    assert!(
        compiled
            .file("headers.py")
            .nodes
            .iter()
            .any(|node| node.form == Form::Call && node.role.is_some_and(|role| role != CallRole::Test))
    );
}

fn copy(from: &std::path::Path, to: &std::path::Path) {
    for entry in std::fs::read_dir(from).expect("read a folder") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            if entry.file_name() == ".unfold" {
                continue;
            }
            std::fs::create_dir_all(&target).expect("create a folder");
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a file");
        }
    }
}

// --- Named child imports -------------------------------------------------------------------

fn activation(sources: &[(&str, &str)], unindexed: &[&str]) -> Compiled {
    compile_files_with(sources, unindexed, &Settings::default()).valid()
}

#[test]
fn named_package_imports_add_exact_child_module_links() {
    let source = "from pkg import first as loaded, second\n";
    let compiled = activation(
        &[
            ("main.py", source),
            ("pkg/__init__.py", "open(\"EXECUTED\", \"w\")\n"),
            ("pkg/first.py", "def first(): pass\n"),
            ("pkg/second/__init__.py", "def second(): pass\n"),
        ],
        &[],
    );
    let rows: Vec<(String, Outcome, String)> = imports(&compiled, "main.py")
        .into_iter()
        .map(|(text, resolution, _, target)| (text, resolution, target))
        .collect();
    assert_eq!(
        rows,
        [
            ("pkg".into(), Outcome::Project, "pkg/__init__.py".into()),
            ("pkg.first".into(), Outcome::Project, "pkg/first.py".into()),
            (
                "pkg.second".into(),
                Outcome::Project,
                "pkg/second/__init__.py".into()
            ),
        ]
    );
    // Each link is written at its specifier: the module, then each child's name.
    let file = compiled.file("main.py");
    let written: Vec<&str> = file
        .links
        .iter()
        .filter(|link| link.kind == LinkKind::Imports)
        .map(|link| {
            assert_eq!(link.from, 0, "a module-level import starts at the module");
            &source[link.range.as_usize()]
        })
        .collect();
    assert_eq!(written, ["pkg", "first", "second"]);
}

#[test]
fn relative_and_namespace_child_imports_keep_unindexed_children() {
    let compiled = activation(
        &[
            ("pkg/__init__.py", ""),
            ("pkg/main.py", "from . import child\n"),
            ("pkg/child.py", ""),
            (
                "main.py",
                "from namespace import child\nfrom pkg import omitted, missing\n",
            ),
            ("namespace/child.py", ""),
        ],
        &["pkg/omitted.py"],
    );
    let rows = |file: &str| -> Vec<(String, Option<Reason>, String)> {
        imports(&compiled, file)
            .into_iter()
            .map(|(text, _, reason, target)| (text, reason, target))
            .collect()
    };
    assert_eq!(
        rows("pkg/main.py"),
        [
            (".".into(), None, "pkg/__init__.py".into()),
            (".child".into(), None, "pkg/child.py".into())
        ]
    );
    assert_eq!(
        rows("main.py"),
        [
            ("namespace".into(), Some(Reason::NotIndexed), String::new()),
            ("namespace.child".into(), None, "namespace/child.py".into()),
            ("pkg".into(), None, "pkg/__init__.py".into()),
            ("pkg.omitted".into(), Some(Reason::NotIndexed), String::new()),
        ]
    );
}

#[test]
fn bound_exports_and_uncertain_package_attributes_never_add_child_imports() {
    for initializer in [
        "child = 1\n",
        "if flag: child = 1\n",
        "from .other import child\n",
        "def __getattr__(name): return 1\n",
        "__path__ = paths\n",
        "__name__ = 'other'\n",
        "__class__ = Custom\n",
        "__dict__ = metadata\n",
        "from external import *\n",
        "exec(code)\n",
    ] {
        let compiled = activation(
            &[
                ("main.py", "from pkg import child\n"),
                ("pkg/__init__.py", initializer),
                ("pkg/child.py", ""),
                ("pkg/other.py", "child = 1\n"),
            ],
            &[],
        );
        assert_eq!(import_texts(&compiled, "main.py"), ["pkg"], "{initializer}");
    }
}

#[test]
fn package_and_namespace_mutations_withhold_child_imports() {
    for namespace in [false, true] {
        for mutation in [
            "pkg.child = replacement",
            "setattr(pkg, 'child', replacement)",
            "setattr(pkg, key, replacement)",
            "pkg.__getattr__ = replacement",
            "pkg.__path__ = paths",
            "pkg.__name__ = 'other'",
            "pkg.__class__ = Custom",
            "pkg.__dict__['child'] = replacement",
            "dictionary = pkg.__dict__\ndictionary['child'] = replacement",
            "pkg.__dict__.update({'child': replacement})",
            "pkg.__path__.insert(0, 'other')",
            "pkg.__path__[:] = ['other']",
            "search = pkg.__path__\nsearch.append('other')",
            "search = pkg.__path__\nsearch += ['other']",
            "insert = pkg.__path__.insert\ninsert(0, 'other')",
            "list.insert(pkg.__path__, 0, 'other')",
            "alias = pkg\nalias.__dict__['child'] = replacement",
            "from builtins import setattr as write\nwrite(pkg, '__name__', 'other')",
            "arguments = (pkg, 'child', replacement)\nsetattr(*arguments)",
        ] {
            let main = format!("import pkg\n{mutation}\nfrom pkg import child\n");
            let mut sources = vec![("main.py", main.as_str()), ("pkg/child.py", "")];
            if !namespace {
                sources.push(("pkg/__init__.py", ""));
            }
            let compiled = activation(&sources, &[]);
            assert!(
                !import_texts(&compiled, "main.py").contains(&"pkg.child".to_owned()),
                "{namespace} {mutation}"
            );
        }
    }
}

#[test]
fn implicit_module_attributes_never_prove_a_child_import() {
    let names = [
        "__name__",
        "__class__",
        "__dict__",
        "__repr__",
        "__getattribute__",
        "__file__",
        "__path__",
        "__spec__",
    ];
    for namespace in [false, true] {
        let main = format!("from pkg import {}\n", names.join(", "));
        let children: Vec<String> = names.iter().map(|name| format!("pkg/{name}.py")).collect();
        let mut sources: Vec<(&str, &str)> = vec![("main.py", main.as_str())];
        sources.extend(children.iter().map(|path| (path.as_str(), "")));
        if !namespace {
            sources.push(("pkg/__init__.py", ""));
        }
        assert_eq!(import_texts(&activation(&sources, &[]), "main.py"), ["pkg"]);
    }
}

#[test]
fn a_packages_own_relative_import_keeps_its_absence_proof() {
    for statement in [
        "from . import child",
        "from . import child as loaded",
        "from pkg import child",
    ] {
        let init = format!("{statement}\n");
        let compiled = activation(&[("pkg/__init__.py", &init), ("pkg/child.py", "")], &[]);
        let children = imports(&compiled, "pkg/__init__.py")
            .into_iter()
            .filter(|row| row.3 == "pkg/child.py")
            .count();
        assert_eq!(children, 1, "{statement}");
    }
    for extra in [
        "child = replacement",
        "if flag: child = replacement",
        "from .other import child",
        "from . import child",
    ] {
        let init = format!("from . import child\n{extra}\n");
        let compiled = activation(
            &[
                ("pkg/__init__.py", &init),
                ("pkg/child.py", ""),
                ("pkg/other.py", "child = replacement\n"),
            ],
            &[],
        );
        assert!(
            !import_texts(&compiled, "pkg/__init__.py").contains(&".child".to_owned()),
            "{extra}"
        );
    }
}

#[test]
fn reads_of_package_metadata_keep_child_imports() {
    for namespace in [false, true] {
        let mut sources = vec![
            (
                "main.py",
                "import pkg as alias\ndictionary = alias.__dict__\nsearch = alias.__path__\nfrom pkg import child as loaded\n",
            ),
            ("pkg/child.py", ""),
        ];
        if !namespace {
            sources.push(("pkg/__init__.py", ""));
        }
        assert!(import_texts(&activation(&sources, &[]), "main.py").contains(&"pkg.child".to_owned()));
    }
}

#[test]
fn package_path_and_global_dictionary_mutations_invalidate_absence() {
    for initializer in [
        "__path__.insert(0, other_path)",
        "__path__[:] = paths",
        "search = __path__\nsearch.append(other_path)",
        "globals()['child'] = replacement",
        "globals().update(child=replacement)",
        "dictionary = globals()\ndictionary['child'] = replacement",
        "from builtins import globals as namespace\nnamespace()['child'] = replacement",
        "locals()['child'] = replacement",
    ] {
        let init = format!("{initializer}\n");
        let compiled = activation(
            &[
                ("main.py", "from pkg import child\n"),
                ("pkg/__init__.py", &init),
                ("pkg/child.py", ""),
            ],
            &[],
        );
        assert_eq!(import_texts(&compiled, "main.py"), ["pkg"], "{initializer}");
    }
    for initializer in [
        "search = __path__\ndictionary = globals()\n",
        "def globals(): return {}\nglobals()['child'] = replacement\n",
    ] {
        let compiled = activation(
            &[
                ("main.py", "from pkg import child\n"),
                ("pkg/__init__.py", initializer),
                ("pkg/child.py", ""),
            ],
            &[],
        );
        assert!(
            import_texts(&compiled, "main.py").contains(&"pkg.child".to_owned()),
            "{initializer}"
        );
    }
}

#[test]
fn relative_child_imports_need_unchanged_importer_metadata() {
    for prefix in [
        "__package__ = 'other'",
        "__package__ = None\nfrom importlib.machinery import ModuleSpec\n__spec__ = ModuleSpec('other.main', loader=None)",
        "globals()['__spec__'].name = 'other.main'",
        "__name__ = 'other.main'",
        "__path__ = ['other']",
        "globals()['__package__'] = 'other'",
        "metadata = globals()\nmetadata.update(__spec__=replacement)",
        "__spec__.name = 'other.main'",
        "def redirect():\n    global __package__\n    __package__ = 'other'",
        "def redirect():\n    globals()['__package__'] = 'other'",
    ] {
        let main = format!("{prefix}\nfrom . import child\n");
        let compiled = activation(
            &[
                ("pkg/__init__.py", ""),
                ("pkg/main.py", &main),
                ("pkg/child.py", ""),
                ("other/__init__.py", ""),
                ("other/child.py", ""),
            ],
            &[],
        );
        let rows = imports(&compiled, "pkg/main.py");
        assert!(!rows.iter().any(|row| row.0 == ".child"), "{prefix}");
        assert_eq!(
            rows.iter()
                .filter(|row| row.0 == ".")
                .map(|row| row.3.as_str())
                .collect::<Vec<_>>(),
            ["pkg/__init__.py"],
            "{prefix}"
        );
    }
    for prefix in [
        "",
        "metadata = globals()",
        "def nested(__package__):\n    __spec__ = replacement\n    __name__ = 'other.main'\n    __path__ = ['other']",
        "class Container:\n    __package__ = 'other'\n    __spec__ = replacement",
        "def nested():\n    locals()['__package__'] = 'other'",
    ] {
        let main = format!("{prefix}\nfrom . import child\n");
        let compiled = activation(
            &[
                ("pkg/__init__.py", ""),
                ("pkg/main.py", &main),
                ("pkg/child.py", ""),
            ],
            &[],
        );
        assert_eq!(
            import_texts(&compiled, "pkg/main.py"),
            [".", ".child"],
            "{prefix}"
        );
    }
}

#[test]
fn qualified_metadata_exports_and_setter_aliases_keep_negative_evidence() {
    for namespace in [false, true] {
        for (storage, mutation) in [
            (
                "import pkg\nstate = pkg.__dict__\n",
                "storage.state['child'] = replacement",
            ),
            (
                "import pkg\nmutate = pkg.__dict__.update\n",
                "storage.mutate(child=replacement)",
            ),
            (
                "import pkg\nstate = pkg.__dict__\nfrom unknown import *\n",
                "storage.state['child'] = replacement",
            ),
            (
                "",
                "import pkg\nsetter = setattr\nsetter = setattr\nsetter(pkg, 'child', replacement)",
            ),
        ] {
            let mutated = format!("import storage\n{mutation}\nfrom pkg import child\n");
            let mut sources = vec![
                ("main.py", mutated.as_str()),
                ("storage.py", storage),
                ("pkg/child.py", ""),
            ];
            if !namespace {
                sources.push(("pkg/__init__.py", ""));
            }
            let found = import_texts(&activation(&sources, &[]), "main.py");
            assert!(!found.contains(&"pkg.child".to_owned()), "{mutation}");
            sources[0].1 = "import storage\nfrom pkg import child\n";
            let found = import_texts(&activation(&sources, &[]), "main.py");
            assert!(found.contains(&"pkg.child".to_owned()), "read-only: {mutation}");
        }
    }
}

#[test]
fn exported_importer_metadata_withholds_only_relative_child_imports() {
    for mutation in [
        "importer.__package__ = 'other'",
        "importer.__dict__['__package__'] = 'other'",
        "importer.__spec__.name = 'other.main'",
        "import storage\nstorage.metadata['__spec__'] = replacement",
    ] {
        let mutated = format!("import pkg.main as importer\n{mutation}\n");
        let compiled = activation(
            &[
                ("pkg/__init__.py", ""),
                (
                    "pkg/main.py",
                    "metadata = globals()\nfrom . import child\nfrom pkg import absolute\n",
                ),
                ("pkg/child.py", ""),
                ("pkg/absolute.py", ""),
                ("storage.py", "from pkg.main import metadata\n"),
                ("mutation.py", &mutated),
            ],
            &[],
        );
        let found = import_texts(&compiled, "pkg/main.py");
        assert!(!found.contains(&".child".to_owned()), "{mutation}");
        assert!(found.contains(&"pkg.absolute".to_owned()), "{mutation}");
    }
}

#[test]
fn type_checking_imports_and_member_reads_add_no_child_imports() {
    let compiled = activation(
        &[
            (
                "main.py",
                "from typing import TYPE_CHECKING\nimport pkg\nif TYPE_CHECKING:\n    from pkg import child\npkg.child\n",
            ),
            ("pkg/__init__.py", ""),
            ("pkg/child.py", ""),
        ],
        &[],
    );
    assert_eq!(import_texts(&compiled, "main.py"), ["typing", "pkg"]);
}

// --- Metaclasses -----------------------------------------------------------------------------

const REPLACING_METACLASS: &str = "def replacement(): return 2\nclass Meta(type):\n    def __getattribute__(cls, name):\n        if name == \"run\": return replacement\n        return type.__getattribute__(cls, name)\ndef register(callback): pass\n";

fn references(compiled: &Compiled) -> usize {
    compiled
        .graph
        .files
        .iter()
        .flat_map(|file| &file.links)
        .filter(|link| link.kind == LinkKind::Passes)
        .count()
}

#[test]
fn custom_metaclass_lookup_withholds_calls_aliases_and_references() {
    let source = format!(
        "{REPLACING_METACLASS}class Commands(metaclass=Meta):\n    @staticmethod\n    def run(): return 1\nclass Inherited(Commands): pass\nAlias = Commands\ncallback = Commands.run\nCommands.run()\nAlias.run()\nInherited.run()\ncallback()\nregister(Commands.run)\nregister(Alias.run)\nregister(Inherited.run)\nregister(callback)\n"
    );
    let compiled = graph(&[("main.py", &source)]);
    for label in ["Commands.run", "Alias.run", "Inherited.run", "callback"] {
        assert_eq!(reason(&compiled, label), Some(Reason::Unsupported), "{label}");
    }
    assert_eq!(references(&compiled), 0);
}

#[test]
fn plain_classes_and_proven_type_metaclasses_keep_member_targets() {
    for header in [
        "class Commands:",
        "class Commands(metaclass=type):",
        "class Commands(metaclass=MetaAlias):",
        "class Commands(metaclass=builtins.type):",
    ] {
        let source = format!(
            "import builtins\nfrom builtins import type as MetaAlias\ndef register(callback): pass\n{header}\n    @staticmethod\n    def run(): return 1\nclass Inherited(Commands): pass\nAlias = Commands\ncallback = Commands.run\nCommands.run()\nAlias.run()\nInherited.run()\ncallback()\nregister(Commands.run)\nregister(callback)\n"
        );
        let compiled = graph(&[("main.py", &source)]);
        for label in ["Commands.run", "Alias.run", "Inherited.run", "callback"] {
            assert!(resolved(&compiled, label), "{header}: {label}");
        }
        assert_eq!(references(&compiled), 2, "{header}");
    }
}

#[test]
fn metaclass_lookup_leaves_instance_and_super_semantics_alone() {
    let source = format!(
        "{REPLACING_METACLASS}class Base(metaclass=Meta):\n    @staticmethod\n    def run(): return 1\n    def dispatch(self): return self.run()\nclass Child(Base):\n    def invoke(self): return super().run()\ndef instance(value: Base):\n    value.run()\n    register(value.run)\n"
    );
    let compiled = graph(&[("main.py", &source)]);
    for label in ["self.run", "super().run", "value.run"] {
        assert!(resolved(&compiled, label), "{label}");
    }
    assert_eq!(references(&compiled), 1);
}

#[test]
fn class_parameters_need_proven_metaclasses_across_descendants() {
    for custom in [false, true] {
        let source = format!(
            "{REPLACING_METACLASS}class Commands:\n    @staticmethod\n    def run(): return 1\n    @classmethod\n    def dispatch(cls):\n        cls.run()\n        register(cls.run)\nclass Child(Commands{}): pass\n",
            if custom { ", metaclass=Meta" } else { "" }
        );
        let compiled = graph(&[("main.py", &source)]);
        let expected = if custom { Some(Reason::Unsupported) } else { None };
        assert_eq!(reason(&compiled, "cls.run"), expected, "{custom}");
        assert_eq!(references(&compiled), usize::from(!custom));
    }
}

#[test]
fn default_source_roots_follow_pyproject_folders_and_src_layouts() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let write = |path: &str, text: &str| {
        let full = root.path().join(path);
        std::fs::create_dir_all(full.parent().expect("a parent")).expect("create folders");
        std::fs::write(full, text).expect("write a file");
    };
    write("svc/pyproject.toml", "[project]\nname = \"svc\"\n");
    write("svc/src/app/__init__.py", "");
    write("svc/src/app/util.py", "def helper(): return 1\n");
    write("svc/src/app/main.py", "from app.util import helper\nhelper()\n");
    write("svc/tools/run.py", "from app.util import helper\nhelper()\n");
    write("loose/main.py", "import app\n");
    let compiled = crate::common::compile_folder_with_threads(root.path(), 1).valid();
    assert_eq!(
        imports(&compiled, "svc/src/app/main.py")[0].3,
        "svc/src/app/util.py"
    );
    assert_eq!(imports(&compiled, "svc/tools/run.py")[0].3, "svc/src/app/util.py");
    assert!(resolved(&compiled, "helper"));
    // Without a pyproject.toml or a src folder above it, the open folder is the root.
    assert_eq!(imports(&compiled, "loose/main.py")[0].1, Outcome::Library);
}

#[test]
fn nonlocal_comprehension_and_class_scopes_follow_python() {
    let compiled = graph(&[(
        "main.py",
        "def outer():\n    def first(): return 1\n    def inner():\n        nonlocal first\n        first = None\n    first()\ndef walrus():\n    def second(): return 1\n    [second := x for x in []]\n    second()\ndef shadow():\n    def third(): return 1\n    [third for third in []]\n    third()\nclass C:\n    def helper(self): return 1\n    def run(self):\n        return helper()\n",
    )]);
    assert_eq!(reason(&compiled, "first"), Some(Reason::MutableBinding));
    assert_eq!(reason(&compiled, "second"), Some(Reason::MutableBinding));
    assert!(
        resolved(&compiled, "third"),
        "a comprehension target binds in its own scope"
    );
    assert_eq!(
        reason(&compiled, "helper"),
        Some(Reason::UnknownName),
        "class scopes are invisible to methods"
    );
}

/// A class body is visible only to code directly in it: not from a method's body behind its
/// type-parameter scope, and not from a nested class body (both are `NameError`s at run time).
/// The type-parameter scope itself sees the class.
#[test]
fn class_scopes_are_invisible_to_generic_methods_and_nested_classes() {
    let compiled = graph(&[(
        "m.py",
        "def f():\n    return 1\n\n\nclass C:\n    helper = f\n\n    def plain(self):\n        return helper()\n\n    def generic[T](self):\n        return helper()\n\n    class Inner:\n        value = helper()\n\n    direct = helper()\n",
    )]);
    let links = calls(&compiled, "helper");
    assert_eq!(links.len(), 4);
    for link in &links[..3] {
        assert_eq!(reason_of(link), Some(Reason::UnknownName), "{link:?}");
    }
    assert_eq!(links[3].to.outcome(), Outcome::Project);
}

/// `nonlocal` binds in the nearest enclosing function where the name is local; a function that
/// itself declares the name `nonlocal` passes it on outwards.
#[test]
fn nonlocal_passes_through_functions_that_declare_it_nonlocal() {
    let compiled = graph(&[(
        "m.py",
        "def a():\n    return 1\n\n\ndef b():\n    return 2\n\n\ndef c():\n    return 3\n\n\ndef outer():\n    x = a\n\n    def f():\n        nonlocal x\n        x = b\n\n        def g():\n            nonlocal x\n            x()\n            x = c\n\n        g()\n\n    f()\n",
    )]);
    assert_eq!(reason(&compiled, "x"), Some(Reason::MutableBinding));
}

/// An expanded base leaves a class's bases unknown, so members aren't looked up through the
/// expanded class.
#[test]
fn expanded_bases_leave_the_linearization_unknown() {
    let compiled = graph(&[(
        "m.py",
        "class Base:\n    def run(self):\n        return 1\n\n\nbases = (Base,)\n\n\nclass C(*bases):\n    pass\n\n\ndef use(c: C):\n    c.run()\n",
    )]);
    assert!(!resolved(&compiled, "c.run"));
}
