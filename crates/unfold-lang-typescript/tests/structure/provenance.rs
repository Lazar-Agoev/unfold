//! Provenance through TypeScript evidence: the bounded import lineage detectors use to
//! recognize framework objects follows imports, `const` factories, and written annotations.

use unfold_engine::ValueRef;
use unfold_engine::provenance::{Provenance, provenance};

use crate::support::Project;

#[test]
fn provenance_follows_imports_factories_and_annotations() {
    let project = Project::new().source(
        "a.ts",
        "import express, { type Express } from 'express';\nimport * as bull from 'bullmq';\nconst app = express();\nconst router = express.Router();\nlet later = express();\nfunction take(server: Express) { server.listen(); }\napp.get('/', take);\nrouter.get('/');\nlater.get('/');\nbull.Queue.name();\n",
    );
    project.link(|program, linked| {
        let file = program.find("a.ts").expect("file");
        let values = program.facts(file).expect("facts").graph.values();
        let lineage = |label: &str| {
            let value = values
                .iter()
                .position(|value| &*value.label == label)
                .unwrap_or_else(|| panic!("no value {label}"));
            provenance(
                program,
                linked,
                ValueRef {
                    file,
                    value: value as u32,
                },
            )
        };
        let expected = |export: &str, path: &[&str]| {
            Some(Provenance {
                module: "express".to_owned(),
                export: export.into(),
                path: path.iter().map(|step| Box::from(*step)).collect(),
            })
        };
        assert_eq!(lineage("app"), expected("default", &["()"]));
        assert_eq!(lineage("router"), expected("default", &["Router", "()"]));
        assert_eq!(lineage("server"), expected("Express", &[":type"]));
        // A `let` factory proves nothing.
        assert_eq!(lineage("later"), None);
        assert_eq!(
            lineage("bull.Queue"),
            Some(Provenance {
                module: "bullmq".to_owned(),
                export: "*".into(),
                path: vec!["Queue".into()],
            })
        );
    });
}
