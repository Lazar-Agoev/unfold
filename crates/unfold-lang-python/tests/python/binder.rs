//! The binder's rules, ported from the proof of concept's binder, import, and metaclass tests:
//! the write census, decorators by identity, receivers and C3, construction, `super()`,
//! reflection, imports and source roots, star imports, and named child imports.

use unfold_core::vocab::{CallRole, Form, LinkKind, Outcome, Reason};
use unfold_core::{Key, Link, LinkTarget};
use unfold_engine::settings::{Settings, SettingsFile};

use crate::common::{Compiled, compile_files, compile_files_with, testdata};

fn graph(sources: &[(&str, &str)]) -> Compiled {
    compile_files(sources, &Settings::default()).valid()
}

fn graph_with(sources: &[(&str, &str)], settings: &str) -> Compiled {
    let settings = SettingsFile::parse(settings).expect("valid settings").settings;
    compile_files(sources, &settings).valid()
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
    // Without a pyproject.toml or a src folder above it, the open folder is the root, which
    // doesn't reach `svc/src/app`: the project package is not found, never a library.
    assert_eq!(imports(&compiled, "loose/main.py")[0].2, Some(Reason::NotFound));
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

const REPO: &str = "from typing import Optional\nimport typing\n\n\nclass Repo:\n    def save(self):\n        return 1\n\n\n";

/// Whether the `calls` link labeled `label` reaches `Repo.save`.
fn saves(compiled: &Compiled, label: &str) -> bool {
    project(call(compiled, label)).is_some_and(|key| key.as_str() == "m.py#Repo.save")
}

/// A call's value is the class its target's written return type names, in exactly the written
/// forms: a class, a class or `None`, a quoted name, and any of these through `await` on an
/// `async def`. Nothing is read from the bodies.
#[test]
fn written_return_types_give_a_call_its_class() {
    let compiled = graph(&[(
        "m.py",
        &format!(
            "{REPO}def plain() -> Repo:\n    ...\n\n\ndef optional() -> Optional[Repo]:\n    ...\n\n\ndef qualified() -> typing.Optional[Repo]:\n    ...\n\n\ndef union() -> Repo | None:\n    ...\n\n\ndef reversed() -> None | Repo:\n    ...\n\n\ndef quoted() -> \"Repo\":\n    ...\n\n\ndef quoted_optional() -> Optional[\"Repo\"]:\n    ...\n\n\nasync def fetch() -> Repo | None:\n    ...\n\n\nclass Service:\n    def factory(self) -> \"Repo\":\n        ...\n\n    def run(self):\n        self.factory().save()\n        made = self.factory()\n        made.save()\n\n\nasync def use():\n    a = plain()\n    a.save()\n    b = optional()\n    b.save()\n    c = qualified()\n    c.save()\n    d = union()\n    d.save()\n    e = reversed()\n    e.save()\n    f = quoted()\n    f.save()\n    g = quoted_optional()\n    g.save()\n    plain().save()\n    fetched = await fetch()\n    fetched.save()\n    a.missing()\n"
        ),
    )]);
    for label in [
        "a.save",
        "b.save",
        "c.save",
        "d.save",
        "e.save",
        "f.save",
        "g.save",
        "plain().save",
        "self.factory().save",
        "made.save",
        "fetched.save",
    ] {
        assert!(saves(&compiled, label), "{label}: {:?}", call(&compiled, label));
    }
    // The receiver is proven, but `Repo` has no such member.
    assert_eq!(reason(&compiled, "a.missing"), Some(Reason::UnknownReceiver));
}

/// Any other annotation leaves the value unknown, as no annotation does: unions of classes,
/// type variables, `Self`, an `Optional` that isn't `typing`'s, an `async def` called without
/// `await`, a plain `def` awaited, and a target that isn't exactly one function. A generic is
/// read by its class, and `list` has no `save`.
#[test]
fn other_return_types_leave_the_value_unknown() {
    let compiled = graph(&[(
        "m.py",
        &format!(
            "{REPO}from typing import Self, TypeVar\n\nT = TypeVar(\"T\")\n\n\nclass Other:\n    def save(self):\n        return 2\n\n    def clone(self) -> Self:\n        ...\n\n\ndef Maybe(value):\n    return value\n\n\ndef generic() -> list[Repo]:\n    ...\n\n\ndef either() -> Repo | Other:\n    ...\n\n\ndef variable() -> T:\n    ...\n\n\ndef unannotated():\n    return Repo()\n\n\ndef local() -> Maybe[Repo]:\n    ...\n\n\ndef nested() -> Optional[Repo | None]:\n    ...\n\n\nasync def fetch() -> Repo:\n    ...\n\n\ndef plain() -> Repo:\n    ...\n\n\ndef rebound() -> Repo:\n    ...\n\n\nrebound = unannotated\n\n\nclass Base:\n    def factory(self) -> Repo:\n        ...\n\n    def run(self):\n        self.factory().save()\n\n\nclass Child(Base):\n    def factory(self) -> Repo:\n        ...\n\n\nasync def use():\n    a = generic()\n    a.save()\n    b = either()\n    b.save()\n    c = variable()\n    c.save()\n    d = Other().clone()\n    d.save()\n    e = unannotated()\n    e.save()\n    f = local()\n    f.save()\n    g = nested()\n    g.save()\n    pending = fetch()\n    pending.save()\n    awaited = await plain()\n    awaited.save()\n    h = rebound()\n    h.save()\n"
        ),
    )]);
    for label in [
        "a.save",
        "b.save",
        "c.save",
        "d.save",
        "e.save",
        "f.save",
        "g.save",
        "pending.save",
        "awaited.save",
        "h.save",
    ] {
        assert_eq!(reason(&compiled, label), Some(Reason::UnknownReceiver), "{label}");
    }
    assert_eq!(reason(&compiled, "self.factory"), Some(Reason::Overridden));
    assert_eq!(
        reason(&compiled, "self.factory().save"),
        Some(Reason::UnknownReceiver)
    );
}

/// The outcome and module label of the `calls` link labeled `label`.
fn library_call(compiled: &Compiled, label: &str) -> (Outcome, Option<String>) {
    let link = call(compiled, label);
    (link.to.outcome(), module(link))
}

/// The standard library's stubs type what the covered modules give: a stub function's written
/// return type (`Self` read as the class it is looked up on), a constructed stub class, a
/// standard-library class written as an annotation (type arguments left out) or as a project
/// function's return type, and literals. Their instances' members are looked up through the
/// stub classes' linearizations and labeled with the class's module.
#[test]
fn stubs_give_standard_library_values_their_class() {
    let compiled = graph(&[(
        "m.py",
        "import asyncio\nimport datetime\nimport logging\nimport os\nimport pathlib\nimport re\nfrom pathlib import Path\nfrom typing import Any\n\nlogger = logging.getLogger(__name__)\n\n\nclass Service:\n    def __init__(self):\n        self.log = logging.getLogger(\"service\")\n        self.items = []\n\n    def run(self):\n        self.log.warning(\"run\")\n        self.items.append(1)\n\n\ndef make() -> logging.Logger:\n    ...\n\n\ndef text() -> str:\n    ...\n\n\ndef table() -> \"dict\":\n    ...\n\n\nasync def main(row: dict, rows: dict[str, Any], name: str, folder: Path):\n    logger.info(\"start\")\n    local = logging.getLogger(__name__)\n    local.debug(\"local\")\n    logging.getLogger().error(\"chained\")\n    re.compile(\"a\").match(\"a\").group()\n    pathlib.Path(\"x\").read_text().strip()\n    Path(\"x\").with_suffix(\".txt\").exists()\n    datetime.datetime.now().isoformat()\n    datetime.UTC.utcoffset(None)\n    os.environ.copy()\n    asyncio.create_task(main({}, {}, \"\", folder)).cancel()\n    row.get(\"k\")\n    rows.get(\"k\")\n    name.upper()\n    folder.iterdir()\n    \"x\".join([])\n    [].append(1)\n    (1, 2).count(1)\n    {}.get(\"k\")\n    {1}.add(2)\n    f\"{name}\".strip()\n    b\"x\".decode()\n    make().info(\"made\")\n    text().upper()\n    table().get(\"k\")\n    str(1).zfill(3)\n    len(name).bit_length()\n",
    )]);
    let library = |label| (Outcome::Library, Some(label));
    let builtin = (Outcome::Builtin, Some("python:builtins"));
    for (label, expected) in [
        ("logger.info", library("python:logging")),
        ("local.debug", library("python:logging")),
        ("logging.getLogger().error", library("python:logging")),
        ("self.log.warning", library("python:logging")),
        ("make().info", library("python:logging")),
        ("re.compile(\"a\").match", library("python:re")),
        ("re.compile(\"a\").match(\"a\").group", library("python:re")),
        ("pathlib.Path(\"x\").read_text", library("python:pathlib")),
        ("Path(\"x\").with_suffix", library("python:pathlib")),
        (
            "Path(\"x\").with_suffix(\".txt\").exists",
            library("python:pathlib"),
        ),
        ("folder.iterdir", library("python:pathlib")),
        ("datetime.datetime.now().isoformat", library("python:datetime")),
        ("datetime.UTC.utcoffset", library("python:datetime")),
        (
            "asyncio.create_task(main({}, {}, \"\", folder)).cancel",
            library("python:_asyncio"),
        ),
        ("pathlib.Path(\"x\").read_text().strip", builtin),
        ("self.items.append", builtin),
        ("row.get", builtin),
        ("rows.get", builtin),
        ("name.upper", builtin),
        ("\"x\".join", builtin),
        ("[].append", builtin),
        ("(1, 2).count", builtin),
        ("{}.get", builtin),
        ("{1}.add", builtin),
        ("f\"{name}\".strip", builtin),
        ("b\"x\".decode", builtin),
        ("text().upper", builtin),
        ("table().get", builtin),
        ("str(1).zfill", builtin),
        ("len(name).bit_length", builtin),
    ] {
        let (outcome, label_of) = library_call(&compiled, label);
        assert_eq!((outcome, label_of.as_deref()), expected, "{label}");
    }
}

/// Anything the stubs don't write stays unknown with its reason: a member the class doesn't
/// have, a return type the overloads disagree on, a stub function read through the wrong
/// `await`, an unannotated parameter, a rebound name, a third-party value, and a member of an
/// attribute read on a class (whose stub types its instances). A class with `__getattr__` makes
/// a member it doesn't declare dynamic.
#[test]
fn values_the_stubs_dont_write_stay_unknown() {
    let compiled = graph(&[(
        "m.py",
        "import asyncio\nimport datetime\nimport logging\nimport os\nimport re\n\nimport fastapi\n\nlogger = logging.getLogger(__name__)\n\n\nasync def main(row):\n    \"x\".missing()\n    logger.missing()\n    os.path.join(\"a\", \"b\").upper()\n    awaited = await re.compile(\"a\")\n    awaited.match(\"a\")\n    pending = asyncio.sleep(1)\n    pending.done()\n    row.get(\"k\")\n    rebound = logging.getLogger()\n    rebound = None\n    rebound.info(\"x\")\n    app = fastapi.FastAPI()\n    app.get(\"/\")\n    datetime.datetime.min.isoformat().upper()\n    list.__class_getitem__(int).anything()\n",
    )]);
    for label in [
        "\"x\".missing",
        "logger.missing",
        "os.path.join(\"a\", \"b\").upper",
        "awaited.match",
        "pending.done",
        "row.get",
        "rebound.info",
        "app.get",
        "datetime.datetime.min.isoformat().upper",
    ] {
        assert_eq!(call(&compiled, label).to.outcome(), Outcome::Unknown, "{label}");
    }
    assert_eq!(reason(&compiled, "\"x\".missing"), Some(Reason::UnknownReceiver));
    assert_eq!(reason(&compiled, "logger.missing"), Some(Reason::UnknownReceiver));
    assert_eq!(reason(&compiled, "row.get"), Some(Reason::UnknownReceiver));
    assert_eq!(reason(&compiled, "app.get"), Some(Reason::UnknownReceiver));
    assert_eq!(
        reason(&compiled, "datetime.datetime.min.isoformat"),
        Some(Reason::UnknownReceiver)
    );
    assert_eq!(
        reason(&compiled, "list.__class_getitem__(int).anything"),
        Some(Reason::Dynamic)
    );
}

/// Only a member the stub class has takes the class's origin: a further hop on a member the
/// stubs write no type for, or a member the class lacks, is unknown. A module's members (a
/// submodule another import loads among them) and anything outside the stubs keep a longer path.
#[test]
fn hops_past_what_the_stubs_type_are_unknown() {
    let compiled = graph(&[(
        "m.py",
        "import logging.handlers\nimport os\nimport re\nimport xml.dom.minidom\nfrom pathlib import Path\n\n\ndef f(p: Path, t: str):\n    m = re.compile(\"a\").match(t)\n    m.string.upper()\n    p.parent.parent.no_such_thing()\n    os.environ.no_such()\n    os.getcwd.no_such()\n    os.environ.get(\"X\")\n    p.with_suffix(\".x\").exists()\n    p.parent.parent.mkdir()\n    p.with_suffix(\".x\").no_such_thing()\n    os.path.join(\"a\")\n    logging.handlers.RotatingFileHandler(\"f\")\n    xml.dom.minidom.parseString(\"x\")\n",
    )]);
    for label in [
        "m.string.upper",
        "p.parent.parent.no_such_thing",
        "os.environ.no_such",
        "os.getcwd.no_such",
        "p.with_suffix(\".x\").no_such_thing",
    ] {
        assert_eq!(reason(&compiled, label), Some(Reason::UnknownReceiver), "{label}");
    }
    let library = |label: &str| (Outcome::Library, Some(label.to_owned()));
    for (label, expected) in [
        ("os.environ.get", library("python:os")),
        ("p.with_suffix(\".x\").exists", library("python:pathlib")),
        // `parent` is written `Self`.
        ("p.parent.parent.mkdir", library("python:pathlib")),
        ("os.path.join", library("python:os")),
        ("logging.handlers.RotatingFileHandler", library("python:logging")),
        ("xml.dom.minidom.parseString", library("python:xml")),
    ] {
        assert_eq!(library_call(&compiled, label), expected, "{label}");
    }
}

/// An instance of `type` is a class the stubs don't name: `type(x)` and a `type[…]` annotation
/// give nothing known, so their members aren't `type`'s.
#[test]
fn instances_of_type_are_unknown() {
    let compiled = graph(&[(
        "m.py",
        "class Node:\n    def __init__(self):\n        self.x = 1\n\n    def clone(self, fresh):\n        type(self).__init__(fresh)\n        type(self).__repr__(fresh)\n        cls = type(self)\n        cls.__init__(fresh)\n\n\ndef make(cls: type[Node], kind: type):\n    cls.__init__(None)\n    kind.mro()\n",
    )]);
    for label in [
        "type(self).__init__",
        "type(self).__repr__",
        "cls.__init__",
        "kind.mro",
    ] {
        for link in calls(&compiled, label) {
            assert_eq!(link.to.outcome(), Outcome::Unknown, "{label}");
        }
    }
    assert_eq!(
        library_call(&compiled, "type"),
        (Outcome::Builtin, Some("python:builtins".to_owned()))
    );
}

/// A written standard-library type is any instance of the class or of a class that derives from
/// it: a member a project class deriving from it defines, or any member of `object` or a
/// protocol, is overridden. A literal or a construction is exactly its class.
#[test]
fn written_library_types_yield_to_project_overrides() {
    let compiled = graph(&[(
        "m.py",
        "import json\nimport logging\nimport os\n\n\nclass MyDict(dict):\n    def get(self, key, default=None):\n        return 3\n\n\nclass Base(logging.Logger):\n    pass\n\n\nclass MyLogger(Base):\n    def info(self, msg, *args, **kwargs):\n        return 1\n\n\nclass Enc(json.JSONEncoder):\n    def encode(self, o):\n        return \"x\"\n\n\nclass Fields(json.JSONDecoder):\n    def __init__(self):\n        self.decode = None\n\n\ndef make() -> logging.Logger:\n    return MyLogger(\"x\")\n\n\ndef f(d: dict, log: logging.Logger, e: json.JSONEncoder, p: os.PathLike, o: object, s: json.JSONDecoder):\n    d.get(\"x\")\n    d.keys()\n    log.info(\"x\")\n    log.debug(\"x\")\n    make().info(\"x\")\n    logging.getLogger().info(\"x\")\n    e.encode(1)\n    p.__fspath__()\n    o.__str__()\n    s.decode(\"x\")\n    {}.get(\"x\")\n    dict().get(\"x\")\n    object().__str__()\n    \"x\".upper()\n",
    )]);
    for label in [
        "d.get",
        "log.info",
        "make().info",
        "logging.getLogger().info",
        "e.encode",
        "p.__fspath__",
        "o.__str__",
        "s.decode",
    ] {
        assert_eq!(reason(&compiled, label), Some(Reason::Overridden), "{label}");
    }
    let builtin = (Outcome::Builtin, Some("python:builtins".to_owned()));
    for (label, expected) in [
        ("d.keys", builtin.clone()),
        ("log.debug", (Outcome::Library, Some("python:logging".to_owned()))),
        ("{}.get", builtin.clone()),
        ("dict().get", builtin.clone()),
        ("object().__str__", builtin.clone()),
        ("\"x\".upper", builtin),
    ] {
        assert_eq!(library_call(&compiled, label), expected, "{label}");
    }
}

/// A name nothing binds is a builtin exactly when the stubs' `builtins` binds it, as a value
/// and as a type; a builtin exception class among a project class's bases is proven through
/// the stubs, aliases included.
#[test]
fn builtins_are_named_by_the_stubs() {
    let compiled = graph(&[(
        "m.py",
        "class Missing(IOError):\n    pass\n\n\ndef f(c, x) -> function:\n    ord(c)\n    id(x)\n    hash(x)\n    divmod(1, 2)\n    frozenset([1]).union([2])\n    vars(x)\n    bytearray(b\"x\")\n    reveal_type(x)\n    raise Missing()\n",
    )]);
    let builtin = (Outcome::Builtin, Some("python:builtins".to_owned()));
    for label in [
        "ord",
        "id",
        "hash",
        "divmod",
        "frozenset",
        "frozenset([1]).union",
        "vars",
        "bytearray",
    ] {
        assert_eq!(library_call(&compiled, label), builtin, "{label}");
    }
    assert_eq!(reason(&compiled, "reveal_type"), Some(Reason::UnknownName));
    assert!(
        project(call(&compiled, "Missing")).is_some(),
        "`IOError` is `OSError`"
    );
    let function = compiled
        .graph
        .files
        .iter()
        .flat_map(|file| &file.links)
        .find(|link| link.kind == LinkKind::Types && &*link.text == "function")
        .expect("the return type");
    assert_eq!(
        reason_of(function),
        Some(Reason::UnknownName),
        "a stub-only class"
    );
}

/// A write that replaces a standard-library member makes every call it may reach `dynamic`:
/// an assignment, `setattr`, or `delattr` on a stub class, on `builtins`, or on an instance the
/// stubs type (which may be any instance of its class). A replaced member reaches the classes
/// that inherit it, and the written types that may hold one of them. A patched module function
/// stays `dynamic`, and members nothing writes keep their proof.
#[test]
fn replaced_standard_library_members_are_dynamic() {
    let cases: [(&str, &[&str]); 7] = [
        (
            "import logging\n\ndef custom(*args): pass\n\nlogging.Logger.info = custom\n\ndef f(log: logging.Logger):\n    logging.getLogger().info(\"x\")\n    log.info(\"x\")\n    logging.Logger.info(log, \"x\")\n",
            &["logging.getLogger().info", "log.info", "logging.Logger.info"],
        ),
        (
            "import builtins\n\ndef custom(*args): pass\n\nbuiltins.print = custom\nprint(\"x\")\n",
            &["print"],
        ),
        (
            "import logging\n\ndef custom(*args): pass\n\nlogger = logging.getLogger(__name__)\nlogger.info = custom\nlogger.info(\"x\")\nlogging.getLogger(\"other\").info(\"y\")\n",
            &["logger.info", "logging.getLogger(\"other\").info"],
        ),
        (
            "import builtins\nimport logging\n\ndef custom(*args): pass\n\nlogger = logging.getLogger()\nsetattr(logging.Logger, \"debug\", custom)\nsetattr(logger, \"warning\", custom)\ndelattr(builtins, \"len\")\nlogger.debug(\"x\")\nlogger.warning(\"x\")\nlen(\"x\")\n",
            &["logger.debug", "logger.warning", "len"],
        ),
        (
            "import logging\n\ndef custom(*args): pass\n\nlogging.Filterer.filter = custom\nlogging.RootLogger.error = custom\nlogger = logging.getLogger()\nlogger.filter(None)\nlogger.error(\"x\")\n",
            &["logger.filter", "logger.error"],
        ),
        (
            "def custom(*args): pass\n\nstr.upper = custom\n\"x\".upper()\nname: str = \"\"\nname.upper()\n",
            &["\"x\".upper", "name.upper"],
        ),
        (
            "import json\n\ndef custom(*args): pass\n\njson.dumps = custom\njson.dumps(1)\n",
            &["json.dumps"],
        ),
    ];
    for (source, replaced) in cases {
        let compiled = graph(&[("m.py", source)]);
        for label in replaced {
            assert_eq!(
                reason(&compiled, label),
                Some(Reason::Dynamic),
                "{label} in {source}"
            );
        }
    }
    // Members and classes nothing writes keep their proof.
    let compiled = graph(&[(
        "m.py",
        "import logging\n\ndef custom(*args): pass\n\nlogging.Logger.info = custom\nlogger = logging.getLogger()\nlogger.warning(\"x\")\nlogging.Formatter().format(None)\nprint(\"x\")\n\"x\".upper()\n",
    )]);
    let library = (Outcome::Library, Some("python:logging".to_owned()));
    let builtin = (Outcome::Builtin, Some("python:builtins".to_owned()));
    assert_eq!(library_call(&compiled, "logger.warning"), library);
    assert_eq!(library_call(&compiled, "logging.Formatter().format"), library);
    assert_eq!(library_call(&compiled, "print"), builtin);
    assert_eq!(library_call(&compiled, "\"x\".upper"), builtin);
    // A replaced builtin class is no builtin type name either.
    let compiled = graph(&[(
        "m.py",
        "import builtins\n\nclass Text:\n    pass\n\nbuiltins.str = Text\n\ndef f(name: str, count: int): pass\n",
    )]);
    let types: Vec<(String, Outcome, Option<Reason>)> = compiled
        .file("m.py")
        .links
        .iter()
        .filter(|link| link.kind == LinkKind::Types)
        .map(|link| (link.text.to_string(), link.to.outcome(), reason_of(link)))
        .collect();
    assert_eq!(
        types,
        [
            ("str".to_owned(), Outcome::Unknown, Some(Reason::Dynamic)),
            ("int".to_owned(), Outcome::Builtin, None),
        ]
    );
}

/// An absolute import that finds nothing is a library only when nothing says it may be project
/// code. When its first part is a module or package the source roots reach, or a Python module
/// or folder of them anywhere in the project (a standard-library name aside), it is
/// `not_found`, and so are the calls through it.
#[test]
fn absolute_misses_under_project_names_are_not_found() {
    let compiled = graph_with(
        &[
            (
                "app/main.py",
                "from pkg.missing import mi\nimport inference\nimport fastapi\nimport io\nimport pkg.real\n\nmi()\ninference.predict()\nfastapi.FastAPI()\nio.StringIO()\npkg.real.run()\n\n\ndef typed(model: inference.Model): pass\n",
            ),
            ("app/pkg/__init__.py", ""),
            ("app/pkg/real.py", "def run(): pass\n"),
            ("ml/inference.py", "def predict(): pass\n"),
            ("tools/io.py", "def helper(): pass\n"),
        ],
        "[[python.project]]\nroot = \"app\"\nsource_roots = [\".\"]\n",
    );
    assert_eq!(
        imports(&compiled, "app/main.py"),
        [
            (
                "pkg.missing".to_owned(),
                Outcome::Unknown,
                Some(Reason::NotFound),
                String::new()
            ),
            (
                "inference".to_owned(),
                Outcome::Unknown,
                Some(Reason::NotFound),
                String::new()
            ),
            ("fastapi".to_owned(), Outcome::Library, None, String::new()),
            ("io".to_owned(), Outcome::Library, None, String::new()),
            (
                "pkg.real".to_owned(),
                Outcome::Project,
                None,
                "app/pkg/real.py".to_owned()
            ),
        ]
    );
    assert_eq!(reason(&compiled, "mi"), Some(Reason::NotFound));
    assert_eq!(reason(&compiled, "inference.predict"), Some(Reason::NotFound));
    assert_eq!(
        library_call(&compiled, "fastapi.FastAPI"),
        (Outcome::Library, Some("fastapi".to_owned()))
    );
    assert_eq!(
        library_call(&compiled, "io.StringIO"),
        (Outcome::Library, Some("python:io".to_owned()))
    );
    assert!(resolved(&compiled, "pkg.real.run"));
    let model = compiled
        .file("app/main.py")
        .links
        .iter()
        .find(|link| link.kind == LinkKind::Types && &*link.text == "inference.Model")
        .expect("the annotation");
    assert_eq!(reason_of(model), Some(Reason::NotFound));
}

/// A written annotation gives a value only as its `types` link reads it: a name an assignment
/// binds (`Alias = Repo`) is its own declaration, never followed, so neither the link nor the
/// value reaches the class. A class the annotation names directly, or through an import, does;
/// a wrapped class's members stay `wrapper`.
#[test]
fn annotations_read_aliases_as_their_types_links_do() {
    let compiled = graph(&[
        (
            "m.py",
            &format!(
                "{REPO}Alias = Repo\n\n\ndef aliased() -> Alias:\n    ...\n\n\ndef direct() -> Repo:\n    ...\n\n\ndef wrap(cls):\n    return cls\n\n\n@wrap\nclass Wrapped:\n    def save(self):\n        return 2\n\n\ndef wrapped() -> Wrapped:\n    ...\n\n\ndef use(param: Alias, imported: Imported):\n    aliased().save()\n    direct().save()\n    wrapped().save()\n    param.save()\n    imported.save()\n    imported_alias().save()\n\n\nfrom other import Imported, imported_alias\n"
            ),
        ),
        (
            "other.py",
            "from m import Repo, Alias\n\nImported = Repo\n\n\ndef imported_alias() -> Alias:\n    ...\n",
        ),
    ]);
    for label in [
        "aliased().save",
        "param.save",
        "imported.save",
        "imported_alias().save",
    ] {
        assert_eq!(reason(&compiled, label), Some(Reason::UnknownReceiver), "{label}");
    }
    assert!(saves(&compiled, "direct().save"));
    assert_eq!(reason(&compiled, "wrapped().save"), Some(Reason::Wrapper));
    let types: Vec<(String, Option<Reason>)> = compiled
        .file("m.py")
        .links
        .iter()
        .filter(|link| link.kind == LinkKind::Types && matches!(&*link.text, "Alias" | "Repo"))
        .map(|link| (link.text.to_string(), reason_of(link)))
        .collect();
    assert!(
        types.contains(&("Alias".to_owned(), Some(Reason::NotConcrete))),
        "{types:?}"
    );
    assert!(types.contains(&("Repo".to_owned(), None)), "{types:?}");
}

/// A generic return type is read by its class, its type arguments dropped, as written variable
/// types are: `-> Repo[str]` gives `Repo`, `-> list[int]` gives `list`, in every form a class
/// is read in. A class that derives from `Generic[T]` stays unresolved, and `type[Repo]` is a
/// class, not an instance.
#[test]
fn generic_return_types_are_read_by_their_class() {
    let compiled = graph(&[(
        "m.py",
        "from typing import Generic, Optional, TypeVar\n\nT = TypeVar(\"T\")\n\n\nclass Repo[T]:\n    def save(self):\n        return 1\n\n\nclass Box(Generic[T]):\n    def open(self):\n        return 2\n\n\ndef make() -> Repo[str]:\n    ...\n\n\ndef maybe() -> Optional[Repo[str]]:\n    ...\n\n\ndef either() -> Repo[str] | None:\n    ...\n\n\nasync def fetch() -> Repo[int]:\n    ...\n\n\ndef rows() -> list[int]:\n    ...\n\n\ndef table() -> dict[str, Repo[str]]:\n    ...\n\n\ndef boxed() -> Box[int]:\n    ...\n\n\ndef kind() -> type[Repo]:\n    ...\n\n\nasync def use():\n    make().save()\n    maybe().save()\n    either().save()\n    (await fetch()).save()\n    rows().append(1)\n    table().get(\"k\")\n    boxed().open()\n    kind().save()\n",
    )]);
    for label in [
        "make().save",
        "maybe().save",
        "either().save",
        "(await fetch()).save",
    ] {
        assert!(
            project(call(&compiled, label)).is_some_and(|key| key.as_str() == "m.py#Repo.save"),
            "{label}: {:?}",
            call(&compiled, label)
        );
    }
    let builtin = (Outcome::Builtin, Some("python:builtins".to_owned()));
    assert_eq!(library_call(&compiled, "rows().append"), builtin);
    assert_eq!(library_call(&compiled, "table().get"), builtin);
    assert_eq!(call(&compiled, "boxed().open").to.outcome(), Outcome::Unknown);
    assert_eq!(call(&compiled, "kind().save").to.outcome(), Outcome::Unknown);
}
