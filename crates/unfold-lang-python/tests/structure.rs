//! Python structure: kinds, roles, names, ranges, and slots for the syntax the mapping covers,
//! checked through full compiles that also pass validation against the snapshots and census.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Compiled, compile_files, testdata};
use unfold_core::vocab::{BlockKind, Role, SlotName};
use unfold_core::{RepoPath, Snapshot};
use unfold_engine::facts::{FileFacts, ValueShape};
use unfold_engine::settings::{PythonVersion, Settings};

/// One compiled file, `main.py`.
struct Main(Compiled);

impl Main {
    fn new(text: &str) -> Self {
        Self(compile_files(&[("main.py", text)], &Settings::default()).valid())
    }

    fn text(&self) -> &str {
        self.0.text("main.py")
    }

    fn slice(&self, range: unfold_core::ByteRange) -> &str {
        &self.text()[range.start as usize..range.end as usize]
    }

    /// Every block but the root as `key (without the path), kind, role, written text`.
    fn blocks(&self) -> Vec<(&str, BlockKind, Option<Role>, &str)> {
        self.0
            .file("main.py")
            .blocks
            .iter()
            .skip(1)
            .map(|block| {
                (
                    block.key.as_str().trim_start_matches("main.py#"),
                    block.kind,
                    block.role,
                    self.slice(block.range),
                )
            })
            .collect()
    }

    fn keys(&self) -> Vec<&str> {
        self.blocks().into_iter().map(|(key, ..)| key).collect()
    }

    fn block(&self, key: &str) -> (&str, BlockKind, Option<Role>, &str) {
        self.blocks()
            .into_iter()
            .find(|block| block.0 == key)
            .unwrap_or_else(|| panic!("no block {key} in {:#?}", self.keys()))
    }

    fn slots(&self, key: &str) -> Vec<(SlotName, Option<u32>, &str)> {
        let file = self.0.file("main.py");
        let local = file
            .blocks
            .iter()
            .find(|block| block.key.as_str() == format!("main.py#{key}"))
            .unwrap_or_else(|| panic!("no block {key}"))
            .local_id;
        file.slots
            .iter()
            .filter(|slot| slot.block == local)
            .map(|slot| (slot.slot, slot.index, self.slice(slot.range)))
            .collect()
    }

    /// The text of every occurrence edge, in block order.
    fn occurrences(&self) -> Vec<&str> {
        self.0
            .file("main.py")
            .edges
            .iter()
            .filter(|edge| edge.kind.is_occurrence())
            .map(|edge| &*edge.text)
            .collect()
    }
}

fn facts(text: &str) -> FileFacts {
    let snapshot = Snapshot::from_text(RepoPath::new("main.py").unwrap(), text).unwrap();
    unfold_lang_python::extract(&snapshot, PythonVersion::V3_14).expect("extracts")
}

#[test]
fn headers_decorated_ranges_control_regions_and_argument_ordinals() {
    let text = "\u{FEFF}def make(): return 1\r\n@make()\r\ndef Kelvin(value=make(), *, named=make()) -> make():\r\n    try:\r\n        with manager() as item:\r\n            match value:\r\n                case 1 if check():\r\n                    return make()\r\n    except ValueError:\r\n        raise\r\n    except TypeError as error:\r\n        return make()\r\n    else:\r\n        make()\r\n    finally:\r\n        make()\r\nKelvin(named=make(), **options)\r\n";
    let main = Main::new(text);
    let (_, kind, _, written) = main.block("function:Kelvin");
    assert_eq!(kind, BlockKind::Function);
    assert!(written.starts_with("@make()\r\ndef Kelvin"));
    let header: Vec<&str> = main
        .blocks()
        .into_iter()
        .filter(|(key, kind, role, _)| {
            key.starts_with("function:Kelvin/call") && *kind == BlockKind::Call && *role == Some(Role::Header)
        })
        .map(|(key, ..)| key)
        .collect();
    assert_eq!(
        header.len(),
        4,
        "the decorator, two defaults, and the return annotation"
    );
    let parameters: Vec<_> = main
        .slots("function:Kelvin")
        .into_iter()
        .filter(|(slot, ..)| *slot == SlotName::Parameter)
        .map(|(_, index, text)| (index, text))
        .collect();
    assert_eq!(parameters, [(Some(0), "value=make()"), (Some(1), "named=make()")]);
    assert_eq!(
        main.slots("function:Kelvin/try[0]"),
        [
            (
                SlotName::Body,
                None,
                &text[text.find(":\r\n        with").unwrap()
                    ..text.find("\r\n    except ValueError").unwrap()]
            ),
            (SlotName::Catch, Some(0), "except ValueError:\r\n        raise"),
            (
                SlotName::Catch,
                Some(1),
                "except TypeError as error:\r\n        return make()"
            ),
            (SlotName::Else, None, ":\r\n        make()"),
            (SlotName::Finally, None, ":\r\n        make()"),
        ]
    );
    assert_eq!(main.block("function:Kelvin/try[0]/catch[0]").2, Some(Role::Catch));
    assert_eq!(
        main.block("function:Kelvin/try[0]/catch[0]/throw[0]").1,
        BlockKind::Throw
    );
    assert_eq!(
        main.slots("function:Kelvin/try[0]/catch[1]"),
        [
            (SlotName::Binding, None, "error"),
            (SlotName::Body, None, ":\r\n        return make()")
        ]
    );
}

#[test]
fn kinds_follow_the_python_mapping() {
    let main = Main::new(concat!(
        "async def run(items):\n",
        "    async for item in items:\n",
        "        if item: break\n",
        "        continue\n",
        "    else:\n",
        "        pass\n",
        "    while False:\n",
        "        pass\n",
        "    else:\n",
        "        done()\n",
        "    async with lock() as held, other():\n",
        "        raise Error() from None\n",
        "    try:\n",
        "        pass\n",
        "    except* ValueError:\n",
        "        pass\n",
        "    handler = lambda event, *rest: event.handle()\n",
        "    total += [square(x) for x in items if keep(x)][0]\n",
        "    limit: int = 3\n",
        "    width: int\n",
        "    type Pair = tuple[int, int]\n",
        "    return {k: convert(v) for k, v in items}\n",
    ));
    assert_eq!(
        main.blocks()
            .into_iter()
            .map(|(key, kind, role, _)| (key, kind, role))
            .collect::<Vec<_>>(),
        [
            ("function:run", BlockKind::Function, Some(Role::Body)),
            ("function:run/for_of[0]", BlockKind::ForOf, Some(Role::Body)),
            ("function:run/for_of[0]/if[0]", BlockKind::If, Some(Role::Body)),
            (
                "function:run/for_of[0]/if[0]/break[0]",
                BlockKind::Break,
                Some(Role::Then)
            ),
            (
                "function:run/for_of[0]/continue[0]",
                BlockKind::Continue,
                Some(Role::Body)
            ),
            ("function:run/while[0]", BlockKind::While, Some(Role::Body)),
            (
                "function:run/while[0]/expression_statement[0]",
                BlockKind::ExpressionStatement,
                Some(Role::Else)
            ),
            (
                "function:run/while[0]/expression_statement[0]/call[0]",
                BlockKind::Call,
                Some(Role::Expression)
            ),
            ("function:run/with[0]", BlockKind::With, Some(Role::Body)),
            (
                "function:run/with[0]/call[0]",
                BlockKind::Call,
                Some(Role::Expression)
            ),
            (
                "function:run/with[0]/call[1]",
                BlockKind::Call,
                Some(Role::Expression)
            ),
            (
                "function:run/with[0]/throw[0]",
                BlockKind::Throw,
                Some(Role::Body)
            ),
            (
                "function:run/with[0]/throw[0]/call[0]",
                BlockKind::Call,
                Some(Role::Expression)
            ),
            ("function:run/try[0]", BlockKind::Try, Some(Role::Body)),
            (
                "function:run/try[0]/catch[0]",
                BlockKind::Catch,
                Some(Role::Catch)
            ),
            (
                "function:run/variable:handler",
                BlockKind::Variable,
                Some(Role::Body)
            ),
            (
                "function:run/variable:handler/function[0]",
                BlockKind::Function,
                Some(Role::Initializer)
            ),
            (
                "function:run/variable:handler/function[0]/call[0]",
                BlockKind::Call,
                Some(Role::Body)
            ),
            (
                "function:run/variable:total",
                BlockKind::Variable,
                Some(Role::Body)
            ),
            (
                "function:run/variable:total/call[0]",
                BlockKind::Call,
                Some(Role::Initializer)
            ),
            (
                "function:run/variable:total/call[1]",
                BlockKind::Call,
                Some(Role::Initializer)
            ),
            (
                "function:run/variable:limit",
                BlockKind::Variable,
                Some(Role::Body)
            ),
            (
                "function:run/variable:width",
                BlockKind::Variable,
                Some(Role::Body)
            ),
            (
                "function:run/variable:Pair",
                BlockKind::Variable,
                Some(Role::Body)
            ),
            ("function:run/return[0]", BlockKind::Return, Some(Role::Body)),
            (
                "function:run/return[0]/call[0]",
                BlockKind::Call,
                Some(Role::Expression)
            ),
        ]
    );
    assert_eq!(
        main.slots("function:run/variable:handler/function[0]"),
        [
            (SlotName::Parameter, Some(0), "event"),
            (SlotName::Parameter, Some(1), "*rest"),
            (SlotName::Body, None, "event.handle()"),
        ]
    );
    assert_eq!(
        main.slots("function:run/with[0]"),
        [
            (SlotName::Expression, None, "lock() as held, other()"),
            (SlotName::Body, None, ":\n        raise Error() from None"),
        ]
    );
    assert_eq!(
        main.slots("function:run/for_of[0]"),
        [
            (SlotName::Initializer, None, "item"),
            (SlotName::Iterable, None, "items"),
            (
                SlotName::Body,
                None,
                ":\n        if item: break\n        continue"
            ),
            (SlotName::Else, None, ":\n        pass"),
        ]
    );
}

#[test]
fn chained_assignments_nest_like_the_proof_of_concept() {
    let main = Main::new("first = (second) = make()\n");
    assert_eq!(
        main.blocks(),
        [
            (
                "variable:(second)",
                BlockKind::Variable,
                Some(Role::Body),
                "first = (second) = make()"
            ),
            (
                "variable:(second)/variable:first",
                BlockKind::Variable,
                Some(Role::Initializer),
                "first = (second) = make()"
            ),
            (
                "variable:(second)/variable:first/call[0]",
                BlockKind::Call,
                Some(Role::Initializer),
                "make()"
            ),
        ]
    );
    assert_eq!(
        main.slots("variable:(second)"),
        // Slots are ordered by start: the inner level starts with the first target.
        [
            (SlotName::Initializer, None, "first = (second) = make()"),
            (SlotName::Name, None, "(second)")
        ]
    );
    assert_eq!(
        main.slots("variable:(second)/variable:first"),
        [
            (SlotName::Name, None, "first"),
            (SlotName::Initializer, None, "make()")
        ]
    );
}

#[test]
fn elif_chains_nest_as_else_branches() {
    let main = Main::new("if a():\n    x = 1\nelif b():\n    y = 2\nelse:\n    z = 3\n");
    assert_eq!(
        main.blocks()
            .into_iter()
            .map(|(key, kind, role, _)| (key, kind, role))
            .collect::<Vec<_>>(),
        [
            ("if[0]", BlockKind::If, Some(Role::Body)),
            ("if[0]/call[0]", BlockKind::Call, Some(Role::Condition)),
            ("if[0]/variable:x", BlockKind::Variable, Some(Role::Then)),
            ("if[0]/if[0]", BlockKind::If, Some(Role::Else)),
            ("if[0]/if[0]/call[0]", BlockKind::Call, Some(Role::Condition)),
            ("if[0]/if[0]/variable:y", BlockKind::Variable, Some(Role::Then)),
            ("if[0]/if[0]/variable:z", BlockKind::Variable, Some(Role::Else)),
        ]
    );
    assert_eq!(
        main.block("if[0]/if[0]").3,
        "elif b():\n    y = 2\nelse:\n    z = 3"
    );
    assert_eq!(
        main.slots("if[0]"),
        [
            (SlotName::Condition, None, "a()"),
            (SlotName::Then, None, ":\n    x = 1"),
            (SlotName::Else, None, "elif b():\n    y = 2\nelse:\n    z = 3"),
        ]
    );
    assert_eq!(
        main.slots("if[0]/if[0]")[2],
        (SlotName::Else, None, ":\n    z = 3")
    );
}

#[test]
fn ranges_keep_the_proof_of_concepts_parentheses() {
    let main = Main::new(
        "if (n := len(items)) > (limit):\n    pass\n(value := compute())\nresult = 1, 2,\nprint((value))\n",
    );
    assert_eq!(main.block("if[0]/variable:n").3, "(n := len(items))");
    assert_eq!(main.block("if[0]/variable:n").2, Some(Role::Condition));
    assert_eq!(
        main.slots("if[0]")[0],
        (SlotName::Condition, None, "(n := len(items)) > (limit)")
    );
    assert_eq!(
        main.slots("if[0]/variable:n"),
        [
            (SlotName::Name, None, "n"),
            (SlotName::Initializer, None, "len(items)")
        ]
    );
    // An assignment expression statement is a variable of its own, not wrapped.
    assert_eq!(main.block("variable:value").3, "(value := compute())");
    // A trailing comma is outside the tuple.
    assert_eq!(main.block("variable:result").3, "result = 1, 2");
    assert_eq!(
        main.slots("expression_statement[0]/call[0]")[1],
        (SlotName::Argument, Some(0), "(value)")
    );
}

#[test]
fn classes_bases_and_members() {
    let main = Main::new(concat!(
        "@register(kind())\n",
        "class Service(base(), metaclass=Meta, **extra):\n",
        "    def __init__(self): pass\n",
        "    @property\n",
        "    def count(self): return 1\n",
        "    @count.setter\n",
        "    def count(self, value): pass\n",
        "    @staticmethod\n",
        "    def build(): return Service()\n",
        "    if flag:\n",
        "        def conditional(self): pass\n",
        "def free(): pass\n",
    ));
    let members: Vec<_> = main
        .blocks()
        .into_iter()
        .map(|(key, kind, role, _)| (key, kind, role))
        .collect();
    assert_eq!(
        members,
        [
            ("class:Service", BlockKind::Class, Some(Role::Body)),
            ("class:Service/call[0]", BlockKind::Call, Some(Role::Header)),
            (
                "class:Service/call[0]/call[0]",
                BlockKind::Call,
                Some(Role::Argument)
            ),
            ("class:Service/call[1]", BlockKind::Call, Some(Role::Header)),
            (
                "class:Service/constructor:__init__",
                BlockKind::Constructor,
                Some(Role::Body)
            ),
            ("class:Service/getter:count", BlockKind::Getter, Some(Role::Body)),
            (
                "class:Service/getter:count/return[0]",
                BlockKind::Return,
                Some(Role::Body)
            ),
            ("class:Service/setter:count", BlockKind::Setter, Some(Role::Body)),
            ("class:Service/method:build", BlockKind::Method, Some(Role::Body)),
            (
                "class:Service/method:build/return[0]",
                BlockKind::Return,
                Some(Role::Body)
            ),
            (
                "class:Service/method:build/return[0]/call[0]",
                BlockKind::Call,
                Some(Role::Expression)
            ),
            ("class:Service/if[0]", BlockKind::If, Some(Role::Body)),
            (
                "class:Service/if[0]/method:conditional",
                BlockKind::Method,
                Some(Role::Then)
            ),
            ("function:free", BlockKind::Function, Some(Role::Body)),
        ]
    );
    assert_eq!(
        main.slots("class:Service"),
        [
            (SlotName::Name, None, "Service"),
            (SlotName::Heritage, None, "base(), metaclass=Meta, **extra"),
        ]
    );
}

#[test]
fn property_needs_proven_identity() {
    let main = Main::new(concat!(
        "def property(fn): return fn\n",
        "class Decoy:\n",
        "    @property\n",
        "    def value(self): return 1\n",
        "import builtins\n",
        "class Real:\n",
        "    @builtins.property\n",
        "    def value(self): return 1\n",
    ));
    assert_eq!(main.block("class:Decoy/method:value").1, BlockKind::Method);
    assert_eq!(main.block("class:Real/getter:value").1, BlockKind::Getter);
}

#[test]
fn names_keep_their_written_spelling() {
    let main = Main::new("ｃｏｕｎｔ = 1\ndef 𐐀receipt(value): return κόστος(value)\nself.total += 1\n");
    assert_eq!(
        main.keys(),
        [
            "variable:ｃｏｕｎｔ",
            "function:𐐀receipt",
            "function:𐐀receipt/return[0]",
            "function:𐐀receipt/return[0]/call[0]",
            "variable:self.total",
        ]
    );
}

#[test]
fn statements_and_expression_statements() {
    let main = Main::new(
        "\"\"\"Docstring.\"\"\"\npass\n...\nawait_me = 1\nglobal g\ndel items[pick()]\nassert check(), why()\n",
    );
    assert_eq!(
        main.blocks()
            .into_iter()
            .map(|(key, kind, role, _)| (key, kind, role))
            .collect::<Vec<_>>(),
        [
            (
                "expression_statement[0]",
                BlockKind::ExpressionStatement,
                Some(Role::Body)
            ),
            (
                "expression_statement[1]",
                BlockKind::ExpressionStatement,
                Some(Role::Body)
            ),
            ("variable:await_me", BlockKind::Variable, Some(Role::Body)),
            ("call[0]", BlockKind::Call, Some(Role::Body)),
            ("call[1]", BlockKind::Call, Some(Role::Body)),
            ("call[2]", BlockKind::Call, Some(Role::Body)),
        ]
    );
}

#[test]
fn match_statements_select_match_and_case() {
    let main = Main::new(
        "match command():\n    case (\"go\" | \"run\") if ready():\n        go()\n    case Point(x=0):\n        pass\n    case _:\n        stop()\n",
    );
    assert_eq!(
        main.slots("match[0]"),
        [
            (SlotName::Expression, None, "command()"),
            (
                SlotName::Body,
                None,
                "case (\"go\" | \"run\") if ready():\n        go()\n    case Point(x=0):\n        pass\n    case _:\n        stop()"
            ),
        ]
    );
    assert_eq!(
        main.slots("match[0]/case[0]"),
        [
            (SlotName::Condition, None, "(\"go\" | \"run\") if ready()"),
            (SlotName::Body, None, ":\n        go()"),
        ]
    );
    assert_eq!(main.block("match[0]/case[0]/call[0]").2, Some(Role::Condition));
    assert_eq!(main.block("match[0]/case[1]").1, BlockKind::Case);
}

#[test]
fn imports_are_runtime_occurrences() {
    let main = Main::new(concat!(
        "import os.path as p, sys\n",
        "from ..pkg import (a, b as c,)\n",
        "from . import sibling\n",
        "from typing import TYPE_CHECKING\n",
        "if TYPE_CHECKING:\n",
        "    import erased\n",
        "    from erased import also\n",
        "else:\n",
        "    import kept\n",
        "def later():\n",
        "    import inside\n",
    ));
    assert_eq!(
        main.blocks()
            .into_iter()
            .map(|(key, _, _, text)| (key, text))
            .collect::<Vec<_>>(),
        [
            ("import[0]", "os.path as p"),
            ("import[1]", "sys"),
            ("import[2]", "from ..pkg import (a, b as c,)"),
            ("import[3]", "from . import sibling"),
            ("import[4]", "from typing import TYPE_CHECKING"),
            (
                "if[0]",
                "if TYPE_CHECKING:\n    import erased\n    from erased import also\nelse:\n    import kept"
            ),
            ("if[0]/import[0]", "kept"),
            ("function:later", "def later():\n    import inside"),
            ("function:later/import[0]", "inside"),
        ]
    );
    assert_eq!(main.slots("import[0]"), [(SlotName::Specifier, None, "os.path")]);
    assert_eq!(main.slots("import[2]"), [(SlotName::Specifier, None, "..pkg")]);
    assert_eq!(main.slots("import[3]"), [(SlotName::Specifier, None, ".")]);
    assert_eq!(main.block("if[0]/import[0]").2, Some(Role::Else));
    assert_eq!(
        main.occurrences(),
        ["os.path", "sys", "..pkg", ".", "typing", "kept", "inside"]
    );
}

#[test]
fn type_checking_needs_proven_identity() {
    let text = std::fs::read_to_string(testdata().join("fixtures/python/policy/imports/guards.py")).unwrap();
    let main = Main::new(&text);
    let imports: Vec<&str> = main
        .blocks()
        .into_iter()
        .filter(|(_, kind, ..)| *kind == BlockKind::Import)
        .map(|(_, _, _, text)| text)
        .collect();
    assert_eq!(
        imports,
        [
            "from typing import TYPE_CHECKING as checking",
            "from .literal_exports import public as runtime_candidate"
        ]
    );
    // A same-file write to the imported identity invalidates it.
    let mutated =
        Main::new("import typing\ntyping.TYPE_CHECKING = True\nif typing.TYPE_CHECKING:\n    import kept\n");
    assert!(mutated.keys().contains(&"if[0]/import[0]"));
}

#[test]
fn literal_import_module_calls_become_imports() {
    let text = std::fs::read_to_string(testdata().join("fixtures/python/policy/imports/dynamic_imports.py"))
        .unwrap();
    let main = Main::new(&text);
    let occurrences: Vec<(BlockKind, &str)> = main
        .blocks()
        .into_iter()
        .filter(|(_, kind, ..)| matches!(kind, BlockKind::Import | BlockKind::Call))
        .map(|(_, kind, _, text)| (kind, text))
        .collect();
    assert_eq!(
        occurrences,
        [
            (BlockKind::Import, "importlib as genuine_importlib"),
            (
                BlockKind::Import,
                "from importlib import import_module as genuine_import_module"
            ),
            (BlockKind::Call, "Importer()"),
            (
                BlockKind::Import,
                "genuine_importlib.import_module(\"imports.literal_exports\")"
            ),
            (
                BlockKind::Import,
                "genuine_import_module(\"imports.literal_exports\")"
            ),
            (
                BlockKind::Call,
                "importlib.import_module(\"imports.literal_exports\")"
            ),
            (BlockKind::Call, "import_module(\"imports.literal_exports\")"),
            (BlockKind::Call, "genuine_import_module(name)"),
        ]
    );
    let key = main
        .blocks()
        .into_iter()
        .find(|(_, kind, _, text)| {
            *kind == BlockKind::Import && text.starts_with("genuine_importlib.import_module")
        })
        .unwrap()
        .0;
    assert_eq!(
        main.slots(key),
        [(SlotName::Specifier, None, "\"imports.literal_exports\"")]
    );
    // Unproven argument binding, a relative name, an expansion, or a non-literal name keeps
    // the ordinary call.
    let unproved = Main::new(
        "import importlib\nimportlib.import_module(package='p', name='x')\nimportlib.import_module('.x', 'p')\nimportlib.import_module(**options)\nimportlib.import_module(f'x')\n",
    );
    let kinds: Vec<BlockKind> = unproved.blocks().into_iter().map(|(_, kind, ..)| kind).collect();
    assert_eq!(kinds.iter().filter(|kind| **kind == BlockKind::Import).count(), 1);
    assert_eq!(kinds.iter().filter(|kind| **kind == BlockKind::Call).count(), 4);
}

#[test]
fn values_keep_written_labels_and_ordinals() {
    let facts = facts("service.repo.save(order, *rest, key=lambda o: o.id, **extra)\nmake(1)(2)\n");
    let calls = facts.graph.calls();
    let values = facts.graph.values();
    let label = |index: u32| &*values[index as usize].label;
    let first = &calls[0];
    assert_eq!(label(first.callee.unwrap()), "service.repo.save");
    let callee = &values[first.callee.unwrap() as usize];
    assert_eq!(callee.shape, ValueShape::Member);
    assert_eq!(callee.member.as_deref(), Some("save"));
    assert_eq!(label(callee.receiver.unwrap()), "service.repo");
    let arguments: Vec<(&str, u32, bool, ValueShape)> = first
        .arguments
        .iter()
        .map(|argument| {
            (
                label(argument.value),
                argument.position,
                argument.reference,
                values[argument.value as usize].shape,
            )
        })
        .collect();
    assert_eq!(
        arguments,
        [
            ("order", 0, true, ValueShape::Name),
            ("rest", 1, false, ValueShape::Name),
            ("lambda o: o.id", 2, true, ValueShape::InlineFunction),
            ("extra", 3, false, ValueShape::Name),
        ]
    );
    let lambda = &values[first.arguments[2].value as usize];
    assert_eq!(
        facts.graph.blocks()[lambda.block.unwrap() as usize].kind,
        BlockKind::Function
    );
    // A call result keeps its written arguments in the label.
    let outer = calls
        .iter()
        .find(|call| label(call.callee.unwrap()) == "make(1)")
        .unwrap();
    assert_eq!(
        values[outer.callee.unwrap() as usize].shape,
        ValueShape::CallResult
    );
}

#[test]
fn with_item_lists_keep_their_parentheses_out() {
    let main = Main::new("with (open(a) as f, open(b) as g):\n    pass\nwith (open(c)) as h:\n    pass\n");
    assert_eq!(
        main.slots("with[0]")[0],
        (SlotName::Expression, None, "open(a) as f, open(b) as g")
    );
    assert_eq!(
        main.slots("with[1]")[0],
        (SlotName::Expression, None, "(open(c)) as h")
    );
}

#[test]
fn the_range_corpus_keeps_unicode_and_crlf_ranges() {
    let text =
        std::fs::read_to_string(testdata().join("fixtures/python-ranges/fixtures/unicode.py")).unwrap();
    let main = Main::new(&text);
    let (_, _, _, decorated) = main.block("function:decorated");
    assert!(decorated.starts_with("@decorator(𐐀receipt)\r\ndef decorated():"));
    assert_eq!(main.block("function:decorated/call[0]").2, Some(Role::Header));
    assert_eq!(
        main.block("function:κόστος").3,
        "def κόστος(ποσό):\r\n    return ποσό"
    );
    assert_eq!(
        main.occurrences(),
        ["κόστος", "𐐀receipt", "decorator", "𐐀receipt"]
    );
}

/// A named child import's specifier is normalized like its `from` module's (no whitespace or
/// line continuations).
#[test]
fn child_import_specifiers_are_normalized() {
    let compiled = compile_files(
        &[
            ("m.py", "from pkg . sub import child\n"),
            ("pkg/__init__.py", ""),
            ("pkg/sub/__init__.py", ""),
            ("pkg/sub/child.py", ""),
        ],
        &Settings::default(),
    )
    .valid();
    let texts: Vec<&str> = compiled
        .file("m.py")
        .edges
        .iter()
        .filter(|edge| edge.kind == unfold_core::vocab::EdgeKind::Imports)
        .map(|edge| &*edge.text)
        .collect();
    assert_eq!(texts, ["pkg.sub", "pkg.sub.child"]);
}
