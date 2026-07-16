use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::driver::{self, Config};
use crate::extract;
use crate::ir::SourcePackage;

use super::system_support::{begin_system_test, command_available};

fn system_test_failure(name: &str, reason: impl std::fmt::Display) -> ! {
    panic!("FAIL {name}: {reason}")
}

fn known_system_headers() -> [&'static str; 4] {
    ["stdint.h", "stdio.h", "linux/stddef.h", "linux/input.h"]
}

fn include_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for variable in ["CPATH", "C_INCLUDE_PATH"] {
        if let Some(value) = env::var_os(variable) {
            for path in env::split_paths(&value).filter(|path| !path.as_os_str().is_empty()) {
                if !roots.contains(&path) {
                    roots.push(path);
                }
            }
        }
    }

    for path in ["/usr/include", "/usr/include/x86_64-linux-gnu"] {
        let path = PathBuf::from(path);
        if !roots.contains(&path) {
            roots.push(path);
        }
    }

    roots
}

fn find_header(header: &str) -> Option<PathBuf> {
    include_search_roots()
        .into_iter()
        .map(|root| root.join(header))
        .find(|candidate| candidate.is_file())
}

fn unique_temp_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    env::temp_dir().join(format!("pac-system-headers-{}", stamp))
}

fn write_wrapper(dir: &Path, header: &str) -> PathBuf {
    let wrapper = dir.join("wrapper.c");
    let source = format!(
        "#include <{}>\nint pac_header_probe(void) {{ return 0; }}\n",
        header
    );
    fs::write(&wrapper, source).expect("writing temporary wrapper");
    wrapper
}

fn parse_header_wrapper(path: &Path) -> Result<(), String> {
    let mut config = Config::with_gcc();
    config.cpp_options.push("-D_GNU_SOURCE".to_owned());
    driver::parse(&config, path)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[test]
fn system_header_wrappers_parse_when_headers_exist() {
    const TEST_NAME: &str = "system_header_wrappers_parse_when_headers_exist";
    let available_headers: Vec<_> = known_system_headers()
        .into_iter()
        .filter(|header| find_header(header).is_some())
        .collect();
    if !begin_system_test(
        TEST_NAME,
        !available_headers.is_empty() && command_available("gcc"),
        "gcc and a known header in CPATH, C_INCLUDE_PATH, or /usr/include",
    ) {
        return;
    }

    let mut attempted = 0usize;
    let mut failures = Vec::new();

    for header in available_headers {
        attempted += 1;
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("creating temporary wrapper directory");
        let wrapper = write_wrapper(&dir, header);

        if let Err(err) = parse_header_wrapper(&wrapper) {
            failures.push(format!("{}: {}", header, err));
        }

        fs::remove_file(&wrapper).expect("removing temporary wrapper");
        fs::remove_dir(&dir).expect("removing temporary wrapper directory");
    }

    assert!(
        attempted > 0,
        "RUN lane selected without an available header"
    );
    if !failures.is_empty() {
        system_test_failure(
            TEST_NAME,
            format!(
                "{} system header wrappers failed:\n{}",
                failures.len(),
                failures.join("\n")
            ),
        );
    }
}

#[test]
fn resilient_parser_recovers_items_from_linux_headers() {
    const TEST_NAME: &str = "resilient_parser_recovers_items_from_linux_headers";
    if !begin_system_test(
        TEST_NAME,
        find_header("linux/input.h").is_some() && command_available("gcc"),
        "gcc and linux/input.h in CPATH, C_INCLUDE_PATH, or /usr/include",
    ) {
        return;
    }

    let dir = unique_temp_dir();
    fs::create_dir_all(&dir).expect("creating temp dir");
    let wrapper = dir.join("wrapper.c");
    fs::write(&wrapper, "#include <linux/input.h>\n").expect("writing wrapper");

    let mut config = Config::with_gcc();
    config.cpp_options.push("-D_GNU_SOURCE".to_owned());

    let processed = match preprocess_for_test(&config, &wrapper) {
        Ok(source) => source,
        Err(error) => {
            let _ = fs::remove_file(&wrapper);
            let _ = fs::remove_dir(&dir);
            system_test_failure(TEST_NAME, error);
        }
    };

    let tu = driver::parse_preprocessed_resilient(&config, processed);
    assert!(
        !tu.unit.0.is_empty(),
        "resilient parser should recover at least some declarations from linux/input.h"
    );

    let _ = fs::remove_file(&wrapper);
    let _ = fs::remove_dir(&dir);
}

fn preprocess_for_test(config: &Config, source: &Path) -> Result<String, String> {
    use std::process::Command;
    let mut cmd = Command::new(&config.cpp_command);
    for item in &config.cpp_options {
        cmd.arg(item);
    }
    cmd.arg(source);
    let output = cmd
        .output()
        .map_err(|error| format!("failed to execute {}: {error}", config.cpp_command))?;
    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|error| format!("preprocessor output: {error}"))
    } else {
        Err(format!(
            "{} exited with {}: {}",
            config.cpp_command,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn parse_wrapper_package(path: &Path) -> Result<SourcePackage, String> {
    let mut config = Config::with_gcc();
    config.cpp_options.push("-D_GNU_SOURCE".to_owned());
    let parsed = driver::parse(&config, path).map_err(|error| error.to_string())?;
    Ok(extract::extract_from_translation_unit(&parsed.unit, None))
}

#[test]
fn openssl_wrapper_extracts_public_surface_when_headers_exist() {
    const TEST_NAME: &str = "openssl_wrapper_extracts_public_surface_when_headers_exist";
    if !begin_system_test(
        TEST_NAME,
        find_header("openssl/ssl.h").is_some() && command_available("gcc"),
        "gcc and OpenSSL development headers",
    ) {
        return;
    }

    let dir = unique_temp_dir();
    fs::create_dir_all(&dir).expect("creating temporary wrapper directory");
    let wrapper = write_wrapper(&dir, "openssl/ssl.h");

    let pkg = parse_wrapper_package(&wrapper).expect("openssl wrapper should parse and extract");

    assert!(pkg.find_function("SSL_new").is_some() || pkg.find_function("SSL_CTX_new").is_some());
    assert!(pkg.find_type_alias("SSL").is_some());
    assert!(pkg.find_type_alias("SSL_CTX").is_some());
    assert!(pkg.item_count() >= 20);

    fs::remove_file(&wrapper).expect("removing temporary wrapper");
    fs::remove_dir(&dir).expect("removing temporary wrapper directory");
}

#[test]
fn openssl_wrapper_extracts_deterministically_when_headers_exist() {
    const TEST_NAME: &str = "openssl_wrapper_extracts_deterministically_when_headers_exist";
    if !begin_system_test(
        TEST_NAME,
        find_header("openssl/ssl.h").is_some() && command_available("gcc"),
        "gcc and OpenSSL development headers",
    ) {
        return;
    }

    let make = || {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("creating temporary wrapper directory");
        let wrapper = write_wrapper(&dir, "openssl/ssl.h");

        let pkg =
            parse_wrapper_package(&wrapper).expect("openssl wrapper should parse and extract");
        let json = serde_json::to_string(&pkg).expect("openssl package json");

        fs::remove_file(&wrapper).expect("removing temporary wrapper");
        fs::remove_dir(&dir).expect("removing temporary wrapper directory");
        json
    };

    assert_eq!(make(), make());
}

#[test]
fn libcurl_wrapper_extracts_public_surface_when_headers_exist() {
    const TEST_NAME: &str = "libcurl_wrapper_extracts_public_surface_when_headers_exist";
    if !begin_system_test(
        TEST_NAME,
        find_header("curl/curl.h").is_some() && command_available("gcc"),
        "gcc and libcurl development headers",
    ) {
        return;
    }

    let dir = unique_temp_dir();
    fs::create_dir_all(&dir).expect("creating temporary wrapper directory");
    let wrapper = write_wrapper(&dir, "curl/curl.h");

    let pkg = parse_wrapper_package(&wrapper).expect("libcurl wrapper should parse and extract");

    assert!(pkg.find_function("curl_easy_init").is_some());
    assert!(pkg.find_function("curl_easy_setopt").is_some());
    assert!(pkg.find_enum("curl_khtype").is_some());
    assert!(pkg.find_type_alias("CURL").is_some() || pkg.find_type_alias("CURLM").is_some());
    assert!(pkg.item_count() >= 40);

    fs::remove_file(&wrapper).expect("removing temporary wrapper");
    fs::remove_dir(&dir).expect("removing temporary wrapper directory");
}

#[test]
fn libcurl_wrapper_extracts_deterministically_when_headers_exist() {
    const TEST_NAME: &str = "libcurl_wrapper_extracts_deterministically_when_headers_exist";
    if !begin_system_test(
        TEST_NAME,
        find_header("curl/curl.h").is_some() && command_available("gcc"),
        "gcc and libcurl development headers",
    ) {
        return;
    }

    let make = || {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("creating temporary wrapper directory");
        let wrapper = write_wrapper(&dir, "curl/curl.h");

        let pkg =
            parse_wrapper_package(&wrapper).expect("libcurl wrapper should parse and extract");
        let json = serde_json::to_string(&pkg).expect("libcurl package json");

        fs::remove_file(&wrapper).expect("removing temporary wrapper");
        fs::remove_dir(&dir).expect("removing temporary wrapper directory");
        json
    };

    assert_eq!(make(), make());
}

#[test]
fn linux_event_loop_wrapper_extracts_combined_surface_when_headers_exist() {
    const TEST_NAME: &str = "linux_event_loop_wrapper_extracts_combined_surface_when_headers_exist";
    let headers = ["sys/epoll.h", "sys/timerfd.h", "sys/signalfd.h"];
    if !begin_system_test(
        TEST_NAME,
        headers.iter().all(|header| find_header(header).is_some()) && command_available("gcc"),
        "gcc and epoll/timerfd/signalfd development headers",
    ) {
        return;
    }

    let dir = unique_temp_dir();
    fs::create_dir_all(&dir).expect("creating temporary wrapper directory");
    let wrapper = dir.join("wrapper.c");
    fs::write(
        &wrapper,
        "#include <sys/epoll.h>\n#include <sys/timerfd.h>\n#include <sys/signalfd.h>\n",
    )
    .expect("writing temporary wrapper");

    let pkg = parse_wrapper_package(&wrapper).expect("combined linux wrapper should parse");

    assert!(pkg.find_function("epoll_create1").is_some());
    assert!(pkg.find_function("timerfd_create").is_some());
    assert!(pkg.find_function("signalfd").is_some());
    assert!(pkg.find_record("epoll_event").is_some());
    assert!(pkg.find_record("signalfd_siginfo").is_some());
    assert!(pkg.item_count() >= 20);

    fs::remove_file(&wrapper).expect("removing temporary wrapper");
    fs::remove_dir(&dir).expect("removing temporary wrapper directory");
}

#[test]
fn linux_event_loop_wrapper_extracts_deterministically_when_headers_exist() {
    const TEST_NAME: &str =
        "linux_event_loop_wrapper_extracts_deterministically_when_headers_exist";
    let headers = ["sys/epoll.h", "sys/timerfd.h", "sys/signalfd.h"];
    if !begin_system_test(
        TEST_NAME,
        headers.iter().all(|header| find_header(header).is_some()) && command_available("gcc"),
        "gcc and epoll/timerfd/signalfd development headers",
    ) {
        return;
    }

    let make = || {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("creating temporary wrapper directory");
        let wrapper = dir.join("wrapper.c");
        fs::write(
            &wrapper,
            "#include <sys/epoll.h>\n#include <sys/timerfd.h>\n#include <sys/signalfd.h>\n",
        )
        .expect("writing temporary wrapper");

        let pkg = parse_wrapper_package(&wrapper).expect("combined linux wrapper should parse");
        let json = serde_json::to_string(&pkg).expect("combined event-loop package json");

        fs::remove_file(&wrapper).expect("removing temporary wrapper");
        fs::remove_dir(&dir).expect("removing temporary wrapper directory");
        json
    };

    assert_eq!(make(), make());
}
