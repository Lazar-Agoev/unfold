//! The graph depends only on the files' contents: not on the order files arrive in, and not on
//! how many threads resolve the links.

use crate::alien::Project;

fn project() -> Project {
    let mut project = Project::new()
        .file(
            "lib/base.alien",
            "(class Base (method ping) (method run (stmt (call (get (this) ping))))) (fn helper (param cb) (call (ref cb)))",
        )
        .file(
            "lib/derived.alien",
            "(use Base \"./base\") (class Derived :base=Base (method ping) (ctor (stmt (super (ref Base)))))",
        )
        .file(
            "app.alien",
            "(use express \"express\" :export=default) (use helper \"./lib/base\") (const app (call (ref express)))
             (fn handle) (stmt (call (get (ref app) post) \"/orders\" (ref handle) (ref helper)))
             (stmt (call (ref helper) (ref handle) (lambda (stmt (call (ref console))))))
             (import \"./lib/derived\") (import \"./gone\")",
        )
        .file("broken.alien", "(fn");
    for index in 0..40 {
        project = project.file(
            &format!("gen/file{index:02}.alien"),
            &format!(
                "(use helper \"../lib/base\") (fn f{index} (param x) (call (ref x)) (call (ref f{index})) (call (ref helper) (ref f{index})))"
            ),
        );
    }
    project
}

#[test]
fn shuffled_files_and_thread_counts_give_identical_exports() {
    let expected = project().compile().valid().canonical();
    assert_eq!(project().reversed().compile().valid().canonical(), expected);
    for threads in [1, 2, 3, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("thread pool");
        let actual = pool.install(|| project().reversed().compile().valid().canonical());
        assert_eq!(actual, expected, "{threads} threads");
    }
    // The export is the whole graph; spot-check that it is not trivially empty.
    assert!(expected.contains("\"to\":\"express\""));
    assert!(expected.contains("\"to\":\"overridden\""));
}
