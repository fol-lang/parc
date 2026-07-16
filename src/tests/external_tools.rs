use std::path::Path;
#[cfg(feature = "system-tests")]
use std::process::Command;
use std::{fs, str};

#[cfg(feature = "system-tests")]
fn run_script(test_name: &str, args: &[&str]) -> Option<String> {
    if !super::system_support::begin_system_test(
        test_name,
        super::system_support::posix_sh_available(),
        "POSIX sh",
    ) {
        return None;
    }

    let output = Command::new("sh")
        .arg("test/full_apps/scripts/refresh_external_fixture.sh")
        .args(args)
        .output()
        .expect("running refresh_external_fixture.sh");

    assert!(
        output.status.success(),
        "script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Some(String::from_utf8(output.stdout).expect("script stdout utf-8"))
}

#[test]
fn refresh_script_is_present() {
    assert!(Path::new("test/full_apps/scripts/refresh_external_fixture.sh").is_file());
}

#[cfg(feature = "system-tests")]
#[test]
fn refresh_script_lists_known_fixtures() {
    let Some(output) = run_script("refresh_script_lists_known_fixtures", &["list"]) else {
        return;
    };
    assert!(output.lines().any(|line| line == "libpng-header"));
    assert!(output.lines().any(|line| line == "musl-stdint"));
    assert!(output.lines().any(|line| line == "zlib-adler32"));
    assert!(output.lines().any(|line| line == "zlib-header"));
    assert!(output.lines().any(|line| line == "zlib-zpipe"));
}

#[cfg(feature = "system-tests")]
#[test]
fn refresh_script_shows_fixture_metadata() {
    let Some(output) = run_script(
        "refresh_script_shows_fixture_metadata",
        &["show", "musl-stdint"],
    ) else {
        return;
    };
    assert!(output.contains("fixture=musl-stdint"));
    assert!(output.contains("project=musl"));
    assert!(output.contains("version=v1.2.5"));
    assert!(output.contains("target=test/full_apps/external/musl/stdint"));
}

#[cfg(feature = "system-tests")]
#[test]
fn refresh_script_shows_zlib_fixture_metadata() {
    let Some(output) = run_script(
        "refresh_script_shows_zlib_fixture_metadata",
        &["show", "zlib-header"],
    ) else {
        return;
    };
    assert!(output.contains("fixture=zlib-header"));
    assert!(output.contains("project=zlib"));
    assert!(output.contains("version=v1.3.1"));
    assert!(output.contains("target=test/full_apps/external/zlib/header"));
}

#[cfg(feature = "system-tests")]
#[test]
fn refresh_script_shows_libpng_fixture_metadata() {
    let Some(output) = run_script(
        "refresh_script_shows_libpng_fixture_metadata",
        &["show", "libpng-header"],
    ) else {
        return;
    };
    assert!(output.contains("fixture=libpng-header"));
    assert!(output.contains("project=libpng"));
    assert!(output.contains("version=v1.6.43"));
    assert!(output.contains("target=test/full_apps/external/libpng/header"));
}

#[cfg(feature = "system-tests")]
#[test]
fn refresh_script_shows_zlib_implementation_metadata() {
    let Some(output) = run_script(
        "refresh_script_shows_zlib_implementation_metadata",
        &["show", "zlib-adler32"],
    ) else {
        return;
    };
    assert!(output.contains("fixture=zlib-adler32"));
    assert!(output.contains("project=zlib"));
    assert!(output.contains("version=v1.3.1"));
    assert!(output.contains("target=test/full_apps/external/zlib/adler32_impl"));
}

#[cfg(feature = "system-tests")]
#[test]
fn refresh_script_shows_zlib_example_metadata() {
    let Some(output) = run_script(
        "refresh_script_shows_zlib_example_metadata",
        &["show", "zlib-zpipe"],
    ) else {
        return;
    };
    assert!(output.contains("fixture=zlib-zpipe"));
    assert!(output.contains("project=zlib"));
    assert!(output.contains("version=v1.3.1"));
    assert!(output.contains("target=test/full_apps/external/zlib/zpipe_example"));
}

#[test]
fn musl_license_note_is_pinned() {
    let path = Path::new("test/full_apps/licenses/musl-MIT.txt");
    assert!(path.is_file());

    let content = fs::read(path).expect("reading musl license file");
    let content = str::from_utf8(&content).expect("license utf-8");
    assert!(content.contains("Permission is hereby granted, free of charge"));
    assert!(content.contains("THE SOFTWARE IS PROVIDED \"AS IS\""));
}

#[test]
fn zlib_license_note_is_pinned() {
    let path = Path::new("test/full_apps/licenses/zlib-License.txt");
    assert!(path.is_file());

    let content = fs::read(path).expect("reading zlib license file");
    let content = str::from_utf8(&content).expect("license utf-8");
    assert!(content.contains("This software is provided 'as-is'"));
    assert!(content.contains("Permission is granted to anyone to use this software"));
}

#[test]
fn libpng_license_note_is_pinned() {
    let path = Path::new("test/full_apps/licenses/libpng-LICENSE.txt");
    assert!(path.is_file());

    let content = fs::read(path).expect("reading libpng license file");
    let content = str::from_utf8(&content).expect("license utf-8");
    assert!(content.contains("PNG Reference Library License version 2"));
    assert!(content.contains("Permission is hereby granted to use, copy, modify, and distribute"));
}
