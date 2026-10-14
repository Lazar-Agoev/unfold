use unfold_core::{RepoPath, Snapshot};

use crate::extract::{ExtractOptions, extract};
use crate::payload::{DeclarationKind, ExportEntry, ImportSource, ModuleFacts, Payload};

fn payload(path: &str, source: &str) -> Payload {
    let snapshot =
        Snapshot::new(RepoPath::new(path).expect("path"), source.as_bytes().to_vec()).expect("text");
    let facts = extract(&snapshot, ExtractOptions::default()).expect("extracts");
    Payload::from_bytes(&facts.payload).expect("payload")
}

fn facts(path: &str, source: &str) -> ModuleFacts {
    payload(path, source).module
}

#[test]
fn module_scope_declarations_name_their_nodes() {
    let source = "export function f() {}\nconst g = () => 1, { a, b: [c] } = o;\nif (x) { var h = 1; let inner = 2; function nested() {} }\nfor (var i of list) {}\nclass K {}\ninterface I {}\ntype T = 1;\nenum E {}\nnamespace N { export const x = 1; }\nnamespace Types { export interface J {} }\ndeclare function d(): void;\n";
    let payload = payload("a.ts", source);
    let facts = &payload.module;
    let kinds: Vec<(&str, DeclarationKind)> = facts
        .declarations
        .iter()
        .map(|declaration| (&*declaration.name, declaration.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("", DeclarationKind::Module),
            ("f", DeclarationKind::Function),
            ("g", DeclarationKind::Const),
            ("a", DeclarationKind::Const),
            ("c", DeclarationKind::Const),
            ("h", DeclarationKind::Var),
            ("i", DeclarationKind::Var),
            ("K", DeclarationKind::Class),
            ("I", DeclarationKind::Interface),
            ("T", DeclarationKind::TypeAlias),
            ("E", DeclarationKind::Enum),
            ("N", DeclarationKind::Namespace { instantiated: true }),
            ("Types", DeclarationKind::Namespace { instantiated: false }),
            ("d", DeclarationKind::Function),
        ]
    );
    // The binding's module declarations come first, aligned: functions with a body, classes,
    // and type declarations have their nodes; variables and an ambient signature don't.
    let node = |index: usize| payload.binding.declarations[index].node;
    assert!(node(1).is_some(), "f");
    assert!(node(2).is_none() && node(3).is_none(), "g, a");
    assert!(
        [7, 8, 9, 10].into_iter().all(|index| node(index).is_some()),
        "K, I, T, E"
    );
    assert_eq!(node(13), None, "d");
    assert!(facts.declarations[1].exported && !facts.declarations[2].exported);
    assert!(facts.module_syntax);
}

#[test]
fn imports_and_exports_are_recorded_as_written() {
    let source = "import d, { a as b, type T } from './m';\nimport * as ns from 'pkg';\nimport type { U } from './t';\nimport r = require('./r');\nimport q = N.M.Q;\nexport import e = N.E;\nexport { b as c, type T as TT };\nexport * from './s';\nexport * as all from './s';\nexport type * from './u';\nexport { x as 'y z' } from './v';\nexport default a.b['c'];\n";
    let facts = facts("a.ts", source);
    let imports: Vec<(String, String, bool, bool)> = facts
        .imports
        .iter()
        .map(|binding| {
            let source = match &binding.source {
                ImportSource::Module { specifier, require } => {
                    format!("{specifier}{}", if *require { " (require)" } else { "" })
                }
                ImportSource::Qualified(names) => names.join("."),
            };
            (
                format!("{} <- {}", binding.local, binding.imported),
                source,
                binding.type_only,
                binding.exported,
            )
        })
        .collect();
    assert_eq!(
        imports,
        [
            ("d <- default".to_owned(), "./m".to_owned(), false, false),
            ("b <- a".to_owned(), "./m".to_owned(), false, false),
            ("T <- T".to_owned(), "./m".to_owned(), true, false),
            ("ns <- *".to_owned(), "pkg".to_owned(), false, false),
            ("U <- U".to_owned(), "./t".to_owned(), true, false),
            (
                "r <- export=".to_owned(),
                "./r (require)".to_owned(),
                false,
                false
            ),
            ("q <- ".to_owned(), "N.M.Q".to_owned(), false, false),
            ("e <- ".to_owned(), "N.E".to_owned(), false, true),
        ]
    );
    assert_eq!(
        facts.exports,
        [
            ExportEntry::Local {
                exported: "c".into(),
                local: "b".into(),
                type_only: false
            },
            ExportEntry::Local {
                exported: "TT".into(),
                local: "T".into(),
                type_only: true
            },
            ExportEntry::Star {
                specifier: "./s".into(),
                type_only: false
            },
            ExportEntry::Reexport {
                exported: "all".into(),
                imported: "*".into(),
                specifier: "./s".into(),
                type_only: false
            },
            ExportEntry::Star {
                specifier: "./u".into(),
                type_only: true
            },
            ExportEntry::Reexport {
                exported: "y z".into(),
                imported: "x".into(),
                specifier: "./v".into(),
                type_only: false
            },
            ExportEntry::Value {
                exported: "default".into(),
                path: Some(vec!["a".into(), "b".into(), "c".into()]),
                declaration: None
            },
        ]
    );
}

#[test]
fn scripts_and_jsx_are_recorded() {
    assert!(!facts("a.ts", "const x = 1;").module_syntax);
    assert!(facts("a.ts", "console.log(import.meta.url);").module_syntax);
    assert!(facts("a.ts", "export = 1;").module_syntax);
    assert!(facts("a.tsx", "const v = <div />;").jsx);
    assert!(!facts("a.tsx", "const v = 1;").jsx);
    let expression = payload("a.ts", "export default (() => 1);");
    let ExportEntry::Value {
        declaration: Some(index),
        ..
    } = expression.module.exports[0]
    else {
        panic!("an expression export: {:?}", expression.module.exports);
    };
    let declaration = &expression.module.declarations[index as usize];
    assert_eq!(declaration.kind, DeclarationKind::Expression);
    assert!(declaration.default);
    // The exported arrow function is the expression declaration's inline function.
    let inline = expression.binding.declarations[index as usize].inline;
    let function = inline.map(|inline| &expression.binding.declarations[inline as usize]);
    assert!(function.and_then(|function| function.node).is_some());
}
