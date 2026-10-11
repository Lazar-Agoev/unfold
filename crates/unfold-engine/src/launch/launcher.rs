//! File launchers (`node`, `tsx`, `ts-node`, `ts-node-esm`, `nodemon`, `bun`) and the exact
//! options each accepts or rejects. Known flags outside the grammar refuse the script instead
//! of turning their operands into module paths; unknown flags are taken as operand-free.

use super::shell::Word;
use super::{LaunchCommand, Refusal};

/// What one launcher step means.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// The step launches a module, written at this index of the step's arguments.
    Launch(LaunchCommand, usize),
    /// The step runs something else (or no module): it has no startup.
    Other,
}

/// The launcher commands, in documentation order.
pub(super) const LAUNCHERS: &[&str] = &["node", "tsx", "ts-node", "ts-node-esm", "nodemon", "bun"];

/// Node (and tsx) options that take a value.
const NODE_VALUES: &[&str] = &[
    "-r",
    "--require",
    "--import",
    "--loader",
    "--experimental-loader",
    "--env-file",
    "-C",
    "--conditions",
    "--input-type",
    "--inspect-port",
    "--title",
];

/// Nodemon options that take a value. `--exec` is listed but rejected before it consumes one.
const NODEMON_VALUES: &[&str] = &["--watch", "-w", "--ext", "-e", "--delay", "--signal", "--exec"];

/// Node and tsx reject these flags.
const NODE_REJECTED: &[&str] = &[
    "-e",
    "--eval",
    "-p",
    "--print",
    "-c",
    "--check",
    "-i",
    "--interactive",
    "-h",
    "--help",
    "-v",
    "--version",
    "--entry-url",
    "--run",
    "--build-sea",
    "--build-snapshot",
    "--build-snapshot-config",
    "--completion-bash",
    "--v8-options",
    "--prof-process",
    "--allow-fs-read",
    "--allow-fs-write",
    "--cpu-prof-dir",
    "--cpu-prof-interval",
    "--cpu-prof-name",
    "--diagnostic-dir",
    "--disable-proto",
    "--disable-warning",
    "--dns-result-order",
    "--env-file-if-exists",
    "--experimental-config-file",
    "--experimental-default-config-file",
    "--experimental-package-map",
    "--experimental-sea-config",
    "--experimental-test-tag-filter",
    "--heap-prof-dir",
    "--heap-prof-interval",
    "--heap-prof-name",
    "--heapsnapshot-near-heap-limit",
    "--heapsnapshot-signal",
    "--icu-data-dir",
    "--debug-port",
    "--inspect-publish-uid",
    "--localstorage-file",
    "--max-http-header-size",
    "--max-old-space-size",
    "--max-old-space-size-percentage",
    "--network-family-autoselection-attempt-timeout",
    "--openssl-config",
    "--redirect-warnings",
    "--report-directory",
    "--report-dir",
    "--report-filename",
    "--report-signal",
    "--secure-heap",
    "--secure-heap-min",
    "--snapshot-blob",
    "--test-concurrency",
    "--test-coverage-branches",
    "--test-coverage-exclude",
    "--test-coverage-functions",
    "--test-coverage-include",
    "--test-coverage-lines",
    "--test-global-setup",
    "--experimental-test-isolation",
    "--test-isolation",
    "--test-name-pattern",
    "--test-random-seed",
    "--test-reporter",
    "--test-reporter-destination",
    "--test-rerun-failures",
    "--test-shard",
    "--test-skip-pattern",
    "--test-timeout",
    "--tls-cipher-list",
    "--tls-keylog",
    "--trace-event-categories",
    "--trace-event-file-pattern",
    "--trace-require-module",
    "--unhandled-rejections",
    "--use-largepages",
    "--v8-pool-size",
    "--watch-kill-signal",
    "--watch-path",
];

/// ts-node and ts-node-esm reject these flags, including the kebab-case spellings of the
/// camelCase long names.
const TS_NODE_REJECTED: &[&str] = &[
    "-e",
    "--eval",
    "-p",
    "--print",
    "-i",
    "--interactive",
    "-h",
    "--help",
    "-v",
    "-vv",
    "-vvv",
    "--version",
    "--showConfig",
    "--show-config",
    "-P",
    "--project",
    "-O",
    "--compilerOptions",
    "--compiler-options",
    "-C",
    "--compiler",
    "--transpiler",
    "-r",
    "--require",
    "--cwd",
    "--scopeDir",
    "--scope-dir",
    "-D",
    "--ignoreDiagnostics",
    "--ignore-diagnostics",
    "-I",
    "--ignore",
    "--experimentalSpecifierResolution",
    "--experimental-specifier-resolution",
];

/// Nodemon rejects these on top of the Node set and Node's value-taking options.
const NODEMON_EXTRA_REJECTED: &[&str] = &[
    "--test",
    "--exec",
    "-x",
    "--cwd",
    "--config",
    "--ignore",
    "-i",
    "--polling-interval",
    "-P",
    "-d",
    "-s",
    "--on-change-only",
    "-C",
    "--dump",
    "-?",
];

/// Bun rejects these flags.
const BUN_REJECTED: &[&str] = &[
    "-e",
    "--eval",
    "-p",
    "--print",
    "-h",
    "--help",
    "-v",
    "--version",
    "--interactive",
    "--workspaces",
    "--parallel",
    "--sequential",
    "--elide-lines",
    "--filter",
    "-F",
    "--shell",
    "--title",
    "--unhandled-rejections",
    "--console-depth",
    "--watch-kill-signal",
    "--inspect",
    "--inspect-wait",
    "--inspect-brk",
    "--preload",
    "-r",
    "--require",
    "--import",
    "--install",
    "--conditions",
    "--main-fields",
    "--extension-order",
    "--tsconfig-override",
    "--define",
    "-d",
    "--drop",
    "--loader",
    "-l",
    "--jsx-factory",
    "--jsx-fragment",
    "--jsx-import-source",
    "--jsx-runtime",
    "--port",
    "--fetch-preconnect",
    "--max-http-header-size",
    "--dns-result-order",
    "--user-agent",
    "--env-file",
    "--cwd",
    "--config",
    "-c",
];

/// Bun subcommands that run tools, not a module.
const BUN_COMMANDS: &[&str] = &[
    "test",
    "build",
    "install",
    "add",
    "remove",
    "update",
    "outdated",
    "link",
    "unlink",
    "pm",
    "x",
    "create",
    "init",
    "upgrade",
    "repl",
    "publish",
    "check",
    "info",
    "patch",
    "patch-commit",
    "audit",
    "help",
    "completions",
];

/// Whether `command` rejects `option` by its exact spelling.
fn rejected(command: &str, option: &str) -> bool {
    match command {
        "node" | "tsx" => NODE_REJECTED.contains(&option),
        "ts-node" | "ts-node-esm" => TS_NODE_REJECTED.contains(&option),
        "bun" => BUN_REJECTED.contains(&option),
        _ => {
            // Nodemon: Node's rejected and value-taking flags plus its own, except the options
            // nodemon itself defines (so `-e` keeps nodemon's extension-list meaning).
            let listed = NODE_REJECTED.contains(&option)
                || NODE_VALUES.contains(&option)
                || NODEMON_EXTRA_REJECTED.contains(&option);
            listed && (option == "--exec" || !NODEMON_VALUES.contains(&option))
        }
    }
}

/// `-<one of letters>` followed by at least one more character on the same line: an attached
/// short-option value such as `-rpreload`.
fn attached(option: &str, letters: &str) -> bool {
    let mut chars = option.chars();
    chars.next() == Some('-')
        && chars.next().is_some_and(|c| letters.contains(c))
        && chars
            .next()
            .is_some_and(|c| !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
}

fn refuses(command: &str, option: &str) -> bool {
    let node_like = matches!(command, "node" | "tsx");
    let ts_node = command.starts_with("ts-node");
    let nodemon = command == "nodemon";
    // Underscore spellings normalize to Node's hyphenated flags.
    let normalized = option.replace('_', "-");
    rejected(command, option)
        || (command == "tsx" && ["--test", "--ignore", "--include", "--exclude"].contains(&option))
        || ((node_like || (nodemon && !NODEMON_VALUES.contains(&option)))
            && (NODE_REJECTED.contains(&normalized.as_str())
                || (normalized != option && NODE_VALUES.contains(&normalized.as_str()))))
        || (!nodemon && attached(option, "ep"))
        || (node_like && attached(option, "rC"))
        || (ts_node && attached(option, "POCDIr"))
        || (nodemon && attached(option, "wexidsPrC"))
}

fn takes_value(command: &str, option: &str) -> bool {
    match command {
        "node" => NODE_VALUES.contains(&option),
        "tsx" => NODE_VALUES.contains(&option) || option == "--tsconfig",
        "nodemon" => NODEMON_VALUES.contains(&option),
        _ => false,
    }
}

/// Interprets one launcher step run from `cwd`. `command` must be one of [`LAUNCHERS`].
pub(super) fn launcher(command: &str, args: &[Word], cwd: &str) -> Result<Step, Refusal> {
    let mut end_of_options = false;
    let mut watch = false;
    let mut index = 0;
    while index < args.len() {
        let text = args[index].literal("launcher option or path")?;
        index += 1;
        if !end_of_options && text == "--" {
            end_of_options = true;
            continue;
        }
        if !end_of_options && text.starts_with('-') && text != "-" {
            let option = text.split_once('=').map_or(text, |(name, _)| name);
            if command == "node" && option == "--test" {
                return Ok(Step::Other);
            }
            if refuses(command, option) {
                return Err(Refusal::new(format!("Unsupported {command} option {option}")));
            }
            if takes_value(command, option) && !text.contains('=') {
                let value = args
                    .get(index)
                    .ok_or_else(|| Refusal::new(format!("Missing value for {option}")))?;
                value.literal(&format!("value of {option}"))?;
                index += 1;
            }
            continue;
        }
        if !end_of_options {
            match (command, text) {
                ("tsx", "watch") if !watch => {
                    watch = true;
                    continue;
                }
                ("node", "inspect") => return Err(Refusal::new("Node inspect mode is unsupported")),
                ("bun", "run") => {
                    return Err(Refusal::new(
                        "bun run options before the script reference are unsupported",
                    ));
                }
                ("bun", _) if BUN_COMMANDS.contains(&text) => return Ok(Step::Other),
                _ => {}
            }
        }
        if text == "-" {
            return Ok(Step::Other);
        }
        if text.is_empty() {
            return Err(Refusal::new("Empty module path"));
        }
        return Ok(Step::Launch(
            LaunchCommand {
                path: text.into(),
                cwd: cwd.into(),
            },
            index - 1,
        ));
    }
    Ok(Step::Other)
}
