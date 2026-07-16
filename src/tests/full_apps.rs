#[cfg(feature = "system-tests")]
use std::env;
use std::io;
#[cfg(feature = "system-tests")]
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "system-tests")]
use crate::driver::{self, Config, Flavor};
#[cfg(feature = "system-tests")]
use crate::env::Env;
use crate::parser;

#[cfg(feature = "system-tests")]
use super::support::collect_fixture_dirs;
use super::support::{manifest_bool, manifest_list_values, manifest_value, read_file};

struct FullAppCase {
    path: PathBuf,
    #[cfg_attr(not(feature = "system-tests"), allow(dead_code))]
    flavor: AppFlavor,
    mode: AppMode,
    expected: AppExpected,
    entry: PathBuf,
    #[cfg_attr(not(feature = "system-tests"), allow(dead_code))]
    include_dirs: Vec<PathBuf>,
    allow_system_includes: bool,
    tags: Vec<String>,
}

#[derive(Copy, Clone, Debug)]
enum AppFlavor {
    Core,
    Gnu,
    Clang,
}

#[derive(Copy, Clone, Debug)]
enum AppMode {
    TranslationUnit,
    Driver,
    Preprocessed,
}

#[derive(Copy, Clone, Debug)]
enum AppExpected {
    ParseOk,
    ParseError,
}

impl FullAppCase {
    fn from_dir(path: PathBuf) -> io::Result<FullAppCase> {
        let manifest_path = path.join("fixture.toml");
        let manifest = read_file(&manifest_path)?;

        let mut flavor = AppFlavor::Core;
        let mut mode = AppMode::TranslationUnit;
        let mut expected = AppExpected::ParseOk;
        let mut entry = None;
        let mut include_dirs = Vec::new();
        let mut allow_system_includes = false;
        let mut tags = Vec::new();

        for line in manifest.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(value) = manifest_value(line, "mode") {
                mode = match value {
                    "translation_unit" => AppMode::TranslationUnit,
                    "driver" => AppMode::Driver,
                    "preprocessed" => AppMode::Preprocessed,
                    _ => panic!("{}: unsupported mode `{}`", manifest_path.display(), value),
                };
            }

            if let Some(value) = manifest_value(line, "flavor") {
                flavor = match value {
                    "core" | "std" => AppFlavor::Core,
                    "gnu" => AppFlavor::Gnu,
                    "clang" => AppFlavor::Clang,
                    _ => panic!(
                        "{}: unsupported flavor `{}`",
                        manifest_path.display(),
                        value
                    ),
                };
            }

            if let Some(value) = manifest_value(line, "entry") {
                entry = Some(PathBuf::from(value));
            }

            if let Some(values) = manifest_list_values(line, "include_dirs") {
                include_dirs = values.into_iter().map(PathBuf::from).collect();
            }

            if let Some(value) = manifest_bool(line, "allow_system_includes") {
                allow_system_includes = value;
            }

            if let Some(values) = manifest_list_values(line, "tags") {
                tags = values;
            }

            if let Some(value) = manifest_value(line, "expected") {
                expected = match value {
                    "parse_ok" => AppExpected::ParseOk,
                    "parse_error" => AppExpected::ParseError,
                    _ => panic!(
                        "{}: unsupported expected outcome `{}`",
                        manifest_path.display(),
                        value
                    ),
                };
            }
        }

        Ok(FullAppCase {
            path,
            flavor,
            mode,
            expected,
            entry: entry.unwrap_or_else(|| PathBuf::from("main.c")),
            include_dirs,
            allow_system_includes,
            tags,
        })
    }

    #[cfg(feature = "system-tests")]
    fn run(&self) -> Result<(), parser::ParseError> {
        let source_path = self.path.join(&self.entry);
        match self.mode {
            AppMode::TranslationUnit => {
                let source = read_file(&source_path).expect("reading full app source");
                let mut env = match self.flavor {
                    AppFlavor::Core => Env::with_core(),
                    AppFlavor::Gnu => Env::with_gnu(),
                    AppFlavor::Clang => Env::with_clang(),
                };
                parser::translation_unit(source.trim_end(), &mut env).map(|_| ())
            }
            AppMode::Driver => {
                let mut config = config_for(self.flavor);
                config.flavor = flavor_for(self.flavor);
                if !self.allow_system_includes {
                    config.cpp_options.push("-nostdinc".to_owned());
                }
                for include_dir in &self.include_dirs {
                    config
                        .cpp_options
                        .push(format!("-I{}", self.path.join(include_dir).display()));
                }
                driver::parse(&config, &source_path)
                    .map(|_| ())
                    .map_err(driver_error_to_parse_error)
            }
            AppMode::Preprocessed => {
                let mut config = config_for(self.flavor);
                config.flavor = flavor_for(self.flavor);
                let source = read_file(&source_path).expect("reading preprocessed full app source");
                driver::parse_preprocessed(&config, source)
                    .map(|_| ())
                    .map_err(syntax_error_to_parse_error)
            }
        }
    }

    fn matches_filters(&self, path_filter: Option<&str>, tag_filter: Option<&str>) -> bool {
        if let Some(path_filter) = path_filter {
            let path = self.path.display().to_string();
            if !path.contains(path_filter) {
                return false;
            }
        }

        if let Some(tag_filter) = tag_filter {
            if !self.tags.iter().any(|tag| tag == tag_filter) {
                return false;
            }
        }

        true
    }

    #[cfg(feature = "system-tests")]
    fn describe(&self) -> String {
        format!(
            "{} [mode={:?}, flavor={:?}, expected={:?}]",
            self.path.display(),
            self.mode,
            self.flavor,
            self.expected
        )
    }
}

#[cfg(feature = "system-tests")]
fn config_for(flavor: AppFlavor) -> Config {
    match flavor {
        AppFlavor::Clang => Config::with_clang(),
        AppFlavor::Core | AppFlavor::Gnu => Config::with_gcc(),
    }
}

#[cfg(feature = "system-tests")]
fn flavor_for(flavor: AppFlavor) -> Flavor {
    match flavor {
        AppFlavor::Core => Flavor::StdC11,
        AppFlavor::Gnu => Flavor::GnuC11,
        AppFlavor::Clang => Flavor::ClangC11,
    }
}

#[cfg(feature = "system-tests")]
fn driver_error_to_parse_error(error: driver::Error) -> parser::ParseError {
    match error {
        driver::Error::SyntaxError(err) => parser::ParseError {
            line: err.line,
            column: err.column,
            offset: err.offset,
            expected: err.expected,
        },
        driver::Error::PreprocessorError(err) => {
            panic!("preprocessor error: {}", err);
        }
    }
}

#[cfg(feature = "system-tests")]
fn syntax_error_to_parse_error(error: driver::SyntaxError) -> parser::ParseError {
    parser::ParseError {
        line: error.line,
        column: error.column,
        offset: error.offset,
        expected: error.expected,
    }
}

#[cfg(feature = "system-tests")]
#[test]
fn full_app_main() {
    const TEST_NAME: &str = "full_app_main";
    if !super::system_support::begin_system_test(
        TEST_NAME,
        super::system_support::command_available("gcc"),
        "gcc",
    ) {
        return;
    }

    let mut case_paths = Vec::new();
    collect_fixture_dirs(Path::new("test/full_apps"), &mut case_paths);
    assert!(
        !case_paths.is_empty(),
        "expected at least one full app fixture"
    );
    let path_filter = env::var("FULL_APP_FILTER").ok();
    let tag_filter = env::var("FULL_APP_TAG").ok();

    let failed = case_paths
        .iter()
        .map(|path| FullAppCase::from_dir(path.to_path_buf()).expect("loading full app fixture"))
        .filter(|case| case.matches_filters(path_filter.as_deref(), tag_filter.as_deref()))
        .filter_map(|case| match (case.expected, case.run()) {
            (AppExpected::ParseOk, Ok(())) => None,
            (AppExpected::ParseError, Err(_)) => None,
            (AppExpected::ParseOk, Err(err)) => Some(format!("{}: {}", case.describe(), err)),
            (AppExpected::ParseError, Ok(())) => Some(format!(
                "{}: expected parse error, but parsing succeeded",
                case.describe()
            )),
        })
        .collect::<Vec<_>>();

    if !failed.is_empty() {
        panic!(
            "{} full app cases failed:\n{}",
            failed.len(),
            failed.join("\n")
        );
    }
}

#[test]
fn full_app_filter_matches_path_and_tag() {
    let case = FullAppCase {
        path: PathBuf::from("test/full_apps/synthetic/single_file/state_machine"),
        flavor: AppFlavor::Gnu,
        mode: AppMode::TranslationUnit,
        expected: AppExpected::ParseOk,
        entry: PathBuf::from("main.c"),
        include_dirs: Vec::new(),
        allow_system_includes: false,
        tags: vec!["synthetic".to_owned(), "single_file".to_owned()],
    };

    assert!(case.matches_filters(None, None));
    assert!(case.matches_filters(Some("state_machine"), None));
    assert!(case.matches_filters(None, Some("synthetic")));
    assert!(case.matches_filters(Some("single_file"), Some("synthetic")));
    assert!(!case.matches_filters(Some("mini_http"), None));
    assert!(!case.matches_filters(None, Some("external")));
}

#[test]
fn full_app_parse_error_expectation_is_supported() {
    let case = FullAppCase {
        path: PathBuf::from("test/full_apps/synthetic/single_file/broken_case"),
        flavor: AppFlavor::Core,
        mode: AppMode::TranslationUnit,
        expected: AppExpected::ParseError,
        entry: PathBuf::from("main.c"),
        include_dirs: Vec::new(),
        allow_system_includes: false,
        tags: vec!["synthetic".to_owned(), "invalid".to_owned()],
    };

    let parse_result: Result<(), parser::ParseError> = Err(parser::ParseError {
        line: 1,
        column: 1,
        offset: 0,
        expected: ::std::collections::HashSet::new(),
    });

    assert!(matches!(
        (case.expected, parse_result),
        (AppExpected::ParseError, Err(_))
    ));
}

#[test]
fn full_app_manifest_supports_preprocessed_mode() {
    let case = FullAppCase::from_dir(PathBuf::from(
        "test/full_apps/synthetic/preprocessed/enum_snapshot",
    ))
    .expect("loading preprocessed full app fixture");

    assert!(matches!(case.mode, AppMode::Preprocessed));
    assert_eq!(case.entry, PathBuf::from("main.i"));
    assert!(case.tags.iter().any(|tag| tag == "preprocessed"));
}

#[test]
fn full_app_manifest_supports_system_include_opt_in() {
    let case = FullAppCase::from_dir(PathBuf::from("test/full_apps/external/zlib/zpipe_example"))
        .expect("loading external zlib example fixture");

    assert!(matches!(case.mode, AppMode::Driver));
    assert!(case.allow_system_includes);
    assert!(case.tags.iter().any(|tag| tag == "example"));
}

#[test]
fn full_app_example_fixtures_can_be_filtered_by_example_tag() {
    let case = FullAppCase::from_dir(PathBuf::from("test/full_apps/external/zlib/zpipe_example"))
        .expect("loading external zlib example fixture");

    assert!(case.matches_filters(None, Some("example")));
    assert!(!case.matches_filters(None, Some("header")));
}

#[test]
fn full_app_external_headers_can_be_tagged_slow() {
    let case = FullAppCase::from_dir(PathBuf::from("test/full_apps/external/musl/stdint"))
        .expect("loading external musl stdint fixture");

    assert!(case.tags.iter().any(|tag| tag == "slow"));
    assert!(case.matches_filters(None, Some("slow")));
}

#[test]
fn full_app_implementation_fixtures_can_be_filtered_by_tag() {
    let case = FullAppCase::from_dir(PathBuf::from("test/full_apps/external/zlib/adler32_impl"))
        .expect("loading external zlib implementation fixture");

    assert!(case.tags.iter().any(|tag| tag == "implementation"));
    assert!(case.matches_filters(None, Some("implementation")));
}
