//! The compiler options Unfold reads from `tsconfig.json`, how one file's options are parsed, and
//! how an `extends` chain merges them.
//!
//! Option names and enumerated values are case-insensitive, as TypeScript reads them. Unknown
//! options are ignored; a known option with a value of the wrong type is reported and ignored.
//! Path-valued options are kept with the folder of the configuration that wrote them (and
//! `${configDir}` is kept unexpanded) until the chain is merged, because both resolve against
//! different configurations.

use serde_json::{Map, Value};

use crate::paths;

/// `module`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ModuleKind {
    None,
    CommonJs,
    Amd,
    Umd,
    System,
    Es2015,
    Es2020,
    Es2022,
    EsNext,
    Node16,
    Node18,
    Node20,
    NodeNext,
    Preserve,
}

impl ModuleKind {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "commonjs" => Self::CommonJs,
            "amd" => Self::Amd,
            "umd" => Self::Umd,
            "system" => Self::System,
            "es6" | "es2015" => Self::Es2015,
            "es2020" => Self::Es2020,
            "es2022" => Self::Es2022,
            "esnext" => Self::EsNext,
            "node16" => Self::Node16,
            "node18" => Self::Node18,
            "node20" => Self::Node20,
            "nodenext" => Self::NodeNext,
            "preserve" => Self::Preserve,
            _ => return None,
        })
    }

    /// One of the Node module kinds (`node16` through `nodenext`).
    pub(crate) const fn is_node(self) -> bool {
        matches!(self, Self::Node16 | Self::Node18 | Self::Node20 | Self::NodeNext)
    }
}

/// `moduleResolution`, or the kind `module` implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Resolution {
    /// `node10` (`node`) and `classic`: CommonJS lookup without package `exports`.
    Node10,
    /// `node16` and `nodenext`: the mode follows each file's module format.
    NodeNext,
    /// `bundler`: package `exports` with the `import` condition, and extension probing.
    Bundler,
}

impl Resolution {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "node" | "node10" | "classic" => Self::Node10,
            "node16" | "nodenext" => Self::NodeNext,
            "bundler" => Self::Bundler,
            _ => return None,
        })
    }
}

/// `moduleDetection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Detection {
    Auto,
    Legacy,
    Force,
}

/// `jsx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Jsx {
    /// `preserve`: `.tsx` emits `.jsx`.
    Preserve,
    /// `react`.
    React,
    /// `react-native`.
    Native,
    /// `react-jsx`, the automatic runtime.
    Automatic,
    /// `react-jsxdev`, the automatic runtime in development mode.
    AutomaticDev,
}

/// `target`, as an ordered level: ES3 is 0, ES5 1, ES2015 2, … ES2025 12, ESNext 99.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Target(pub(crate) u8);

impl Target {
    pub(crate) const ES2015: Self = Self(2);
    pub(crate) const ES2017: Self = Self(4);
    pub(crate) const ES2020: Self = Self(7);
    pub(crate) const ES2022: Self = Self(9);
    pub(crate) const ESNEXT: Self = Self(99);

    fn parse(value: &str) -> Option<Self> {
        Some(Self(match value {
            "es3" => 0,
            "es5" => 1,
            "es6" | "es2015" => 2,
            "esnext" => 99,
            _ => {
                let year: u16 = value.strip_prefix("es")?.parse().ok()?;
                u8::try_from(year.checked_sub(2013)?)
                    .ok()
                    .filter(|level| (3..99).contains(level))?
            }
        }))
    }
}

/// A path written in a configuration file, with the folder of the file that wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placed {
    pub(crate) value: String,
    pub(crate) dir: String,
}

impl Placed {
    /// The folder-relative path this names for a configuration in `leaf_dir` (which `${configDir}`
    /// stands for); `None` outside the folder.
    pub(crate) fn resolve(&self, leaf_dir: &str) -> Option<String> {
        resolve_value(&self.value, &self.dir, leaf_dir)
    }
}

/// A written path resolved from `dir`, with `${configDir}` meaning `leaf_dir`.
pub(crate) fn resolve_value(value: &str, dir: &str, leaf_dir: &str) -> Option<String> {
    match value.strip_prefix("${configDir}") {
        Some(rest) => paths::join(leaf_dir, rest.trim_start_matches(['/', '\\'])),
        None => paths::join(dir, value),
    }
}

/// `paths`: patterns and their substitutions, with the folder of the file that wrote them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathMap {
    pub(crate) entries: Vec<(String, Vec<String>)>,
    pub(crate) dir: String,
}

/// The options of one configuration file, or of a merged chain. Every field is optional so a
/// chain can merge them: the extending file's value wins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CompilerOptions {
    pub(crate) module: Option<ModuleKind>,
    pub(crate) module_resolution: Option<Resolution>,
    pub(crate) module_detection: Option<Detection>,
    pub(crate) jsx: Option<Jsx>,
    pub(crate) target: Option<Target>,
    pub(crate) use_define_for_class_fields: Option<bool>,
    pub(crate) verbatim_module_syntax: Option<bool>,
    pub(crate) allow_js: Option<bool>,
    pub(crate) resolve_json_module: Option<bool>,
    pub(crate) allow_synthetic_default_imports: Option<bool>,
    pub(crate) es_module_interop: Option<bool>,
    pub(crate) custom_conditions: Option<Vec<String>>,
    pub(crate) base_url: Option<Placed>,
    pub(crate) paths: Option<PathMap>,
}

impl CompilerOptions {
    /// The options of a TypeScript source with no `tsconfig.json`: ESNext, NodeNext.
    pub(crate) fn without_config() -> Self {
        Self {
            module: Some(ModuleKind::NodeNext),
            module_resolution: Some(Resolution::NodeNext),
            target: Some(Target::ESNEXT),
            ..Self::default()
        }
    }

    /// These options over `base`: each option this chain link sets wins.
    pub(crate) fn over(self, base: &Self) -> Self {
        Self {
            module: self.module.or(base.module),
            module_resolution: self.module_resolution.or(base.module_resolution),
            module_detection: self.module_detection.or(base.module_detection),
            jsx: self.jsx.or(base.jsx),
            target: self.target.or(base.target),
            use_define_for_class_fields: self
                .use_define_for_class_fields
                .or(base.use_define_for_class_fields),
            verbatim_module_syntax: self.verbatim_module_syntax.or(base.verbatim_module_syntax),
            allow_js: self.allow_js.or(base.allow_js),
            resolve_json_module: self.resolve_json_module.or(base.resolve_json_module),
            allow_synthetic_default_imports: self
                .allow_synthetic_default_imports
                .or(base.allow_synthetic_default_imports),
            es_module_interop: self.es_module_interop.or(base.es_module_interop),
            custom_conditions: self.custom_conditions.or_else(|| base.custom_conditions.clone()),
            base_url: self.base_url.or_else(|| base.base_url.clone()),
            paths: self.paths.or_else(|| base.paths.clone()),
        }
    }

    /// The module resolution these options select: `moduleResolution`, or the kind `module`
    /// implies (Node kinds resolve like NodeNext, `commonjs` like node10, anything else like a
    /// bundler, as TypeScript 6 defaults).
    pub(crate) fn resolution(&self) -> Resolution {
        self.module_resolution.unwrap_or(match self.module {
            Some(module) if module.is_node() => Resolution::NodeNext,
            Some(ModuleKind::CommonJs) => Resolution::Node10,
            _ => Resolution::Bundler,
        })
    }

    /// The module detection in effect: Node module kinds default to `force` (as the proof of
    /// concept's TypeScript fork does), everything else to `auto`.
    pub(crate) fn detection(&self) -> Detection {
        self.module_detection.unwrap_or(match self.module {
            Some(module) if module.is_node() => Detection::Force,
            _ => Detection::Auto,
        })
    }

    /// The target, with an unspecified (or removed ES3) target meaning the latest standard.
    pub(crate) fn effective_target(&self) -> Target {
        match self.target {
            None | Some(Target(0)) => Target::ESNEXT,
            Some(target) => target,
        }
    }
}

/// One configuration file as written: its options and `extends`.
#[derive(Debug, Clone, Default)]
pub(crate) struct RawConfig {
    pub(crate) extends: Vec<String>,
    pub(crate) options: CompilerOptions,
}

/// Something wrong with a configuration file's contents.
pub(crate) type Complaint = String;

/// Parses one configuration file written in folder `dir`. Wrong-typed values are complained
/// about and skipped.
pub(crate) fn parse_config(value: &Value, dir: &str, complaints: &mut Vec<Complaint>) -> RawConfig {
    let Some(root) = value.as_object() else {
        complaints.push("the configuration is not a JSON object".to_owned());
        return RawConfig::default();
    };
    let mut config = RawConfig::default();
    for (key, value) in root {
        match key.as_str() {
            "extends" => match value {
                Value::String(base) => config.extends = vec![base.clone()],
                Value::Array(items) => {
                    config.extends = strings(items, "extends", complaints);
                }
                _ => complaints.push("`extends` must be a string or an array of strings".to_owned()),
            },
            "compilerOptions" => match value.as_object() {
                Some(options) => config.options = parse_options(options, dir, complaints),
                None => complaints.push("`compilerOptions` must be an object".to_owned()),
            },
            _ => {}
        }
    }
    config
}

fn strings(items: &[Value], key: &str, complaints: &mut Vec<Complaint>) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item.as_str() {
            Some(text) => Some(text.to_owned()),
            None => {
                complaints.push(format!("`{key}` must contain only strings"));
                None
            }
        })
        .collect()
}

fn parse_options(
    options: &Map<String, Value>,
    dir: &str,
    complaints: &mut Vec<Complaint>,
) -> CompilerOptions {
    let mut parsed = CompilerOptions::default();
    for (key, value) in options {
        let name = key.to_ascii_lowercase();
        let mut wrong =
            |expected: &str| complaints.push(format!("`compilerOptions.{key}` must be {expected}"));
        let flag = || value.as_bool();
        let enumerated = || value.as_str().map(str::to_ascii_lowercase);
        let placed = || {
            value.as_str().map(|text| Placed {
                value: text.to_owned(),
                dir: dir.to_owned(),
            })
        };
        match name.as_str() {
            "module" => set(
                &mut parsed.module,
                enumerated().and_then(|v| ModuleKind::parse(&v)),
                &mut wrong,
                "a module kind",
            ),
            "moduleresolution" => set(
                &mut parsed.module_resolution,
                enumerated().and_then(|v| Resolution::parse(&v)),
                &mut wrong,
                "a module resolution kind",
            ),
            "moduledetection" => set(
                &mut parsed.module_detection,
                enumerated().and_then(|v| match v.as_str() {
                    "auto" => Some(Detection::Auto),
                    "legacy" => Some(Detection::Legacy),
                    "force" => Some(Detection::Force),
                    _ => None,
                }),
                &mut wrong,
                "`auto`, `legacy`, or `force`",
            ),
            "jsx" => set(
                &mut parsed.jsx,
                enumerated().and_then(|v| match v.as_str() {
                    "preserve" => Some(Jsx::Preserve),
                    "react" => Some(Jsx::React),
                    "react-native" => Some(Jsx::Native),
                    "react-jsx" => Some(Jsx::Automatic),
                    "react-jsxdev" => Some(Jsx::AutomaticDev),
                    _ => None,
                }),
                &mut wrong,
                "a JSX mode",
            ),
            "target" => set(
                &mut parsed.target,
                enumerated().and_then(|v| Target::parse(&v)),
                &mut wrong,
                "a target",
            ),
            "usedefineforclassfields" => set(
                &mut parsed.use_define_for_class_fields,
                flag(),
                &mut wrong,
                "a boolean",
            ),
            "verbatimmodulesyntax" => set(
                &mut parsed.verbatim_module_syntax,
                flag(),
                &mut wrong,
                "a boolean",
            ),
            "allowjs" => set(&mut parsed.allow_js, flag(), &mut wrong, "a boolean"),
            "resolvejsonmodule" => set(&mut parsed.resolve_json_module, flag(), &mut wrong, "a boolean"),
            "allowsyntheticdefaultimports" => {
                set(
                    &mut parsed.allow_synthetic_default_imports,
                    flag(),
                    &mut wrong,
                    "a boolean",
                );
            }
            "esmoduleinterop" => set(&mut parsed.es_module_interop, flag(), &mut wrong, "a boolean"),
            "customconditions" => {
                let list = value.as_array().and_then(|items| {
                    items
                        .iter()
                        .map(|item| item.as_str().map(str::to_owned))
                        .collect()
                });
                set(
                    &mut parsed.custom_conditions,
                    list,
                    &mut wrong,
                    "an array of strings",
                );
            }
            "baseurl" => set(&mut parsed.base_url, placed(), &mut wrong, "a string"),
            "paths" => {
                let entries = value.as_object().and_then(|map| {
                    map.iter()
                        .map(|(pattern, substitutions)| {
                            let list = substitutions
                                .as_array()?
                                .iter()
                                .map(|item| item.as_str().map(str::to_owned))
                                .collect::<Option<Vec<_>>>()?;
                            Some((pattern.clone(), list))
                        })
                        .collect::<Option<Vec<_>>>()
                });
                let map = entries.map(|entries| PathMap {
                    entries,
                    dir: dir.to_owned(),
                });
                set(&mut parsed.paths, map, &mut wrong, "an object of string arrays");
            }
            _ => {}
        }
    }
    parsed
}

fn set<T>(slot: &mut Option<T>, value: Option<T>, wrong: &mut impl FnMut(&str), expected: &str) {
    match value {
        Some(value) => *slot = Some(value),
        None => wrong(expected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str, dir: &str) -> (RawConfig, Vec<Complaint>) {
        let mut complaints = Vec::new();
        let value = crate::config::jsonc::parse(text).expect("jsonc");
        (parse_config(&value, dir, &mut complaints), complaints)
    }

    #[test]
    fn reads_options_case_insensitively() {
        let (config, complaints) = parse(
            r#"{"extends": ["./a.json", "pkg"], "compilerOptions": {"Module": "NodeNext", "TARGET": "ES2022",
                "verbatimModuleSyntax": true, "baseUrl": "./src", "paths": {"@/*": ["./*"]},
                "jsx": "react-jsx", "unknown": 1}, "include": ["src"]}"#,
            "app",
        );
        assert!(complaints.is_empty(), "{complaints:?}");
        assert_eq!(config.extends, ["./a.json", "pkg"]);
        let options = &config.options;
        assert_eq!(options.module, Some(ModuleKind::NodeNext));
        assert_eq!(options.target, Some(Target::ES2022));
        assert_eq!(options.verbatim_module_syntax, Some(true));
        assert_eq!(options.jsx, Some(Jsx::Automatic));
        assert_eq!(
            options
                .base_url
                .as_ref()
                .and_then(|url| url.resolve("app"))
                .as_deref(),
            Some("app/src")
        );
        assert_eq!(
            options.paths.as_ref().map(|paths| paths.dir.as_str()),
            Some("app")
        );
        assert_eq!(options.resolution(), Resolution::NodeNext);
        assert_eq!(options.detection(), Detection::Force);
    }

    #[test]
    fn complains_about_wrong_types_and_merges_chains() {
        let (config, complaints) = parse(
            r#"{"compilerOptions": {"module": 3, "allowJs": "yes"}, "extends": 1}"#,
            "",
        );
        assert_eq!(complaints.len(), 3, "{complaints:?}");
        assert_eq!(config.options, CompilerOptions::default());
        let base = CompilerOptions {
            module: Some(ModuleKind::CommonJs),
            allow_js: Some(true),
            ..CompilerOptions::default()
        };
        let child = CompilerOptions {
            module: Some(ModuleKind::EsNext),
            ..CompilerOptions::default()
        };
        let merged = child.over(&base);
        assert_eq!(
            (merged.module, merged.allow_js),
            (Some(ModuleKind::EsNext), Some(true))
        );
        assert_eq!(merged.resolution(), Resolution::Bundler);
        assert_eq!(merged.detection(), Detection::Auto);
        assert_eq!(base.resolution(), Resolution::Node10);
        assert_eq!(Target::parse("es2025"), Some(Target(12)));
        assert_eq!(Target::parse("es1999"), None);
        assert_eq!(CompilerOptions::default().effective_target(), Target::ESNEXT);
    }
}
