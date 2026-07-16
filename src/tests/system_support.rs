use std::env;

const SYSTEM_TEST_MODE_ENV: &str = "PARC_SYSTEM_TEST_MODE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SystemTestMode {
    Optional,
    Required,
}

fn system_test_mode() -> SystemTestMode {
    match env::var(SYSTEM_TEST_MODE_ENV).as_deref() {
        Ok("required") => SystemTestMode::Required,
        Ok("optional") | Err(_) => SystemTestMode::Optional,
        Ok(value) => {
            panic!("{SYSTEM_TEST_MODE_ENV} must be 'optional' or 'required', got '{value}'")
        }
    }
}

pub(crate) fn begin_system_test(name: &str, available: bool, prerequisite: &str) -> bool {
    if available {
        eprintln!("RUN {name}");
        return true;
    }

    match system_test_mode() {
        SystemTestMode::Optional => {
            eprintln!("SKIP {name}: missing {prerequisite}");
            false
        }
        SystemTestMode::Required => {
            panic!("FAIL {name}: missing required prerequisite: {prerequisite}")
        }
    }
}

pub(crate) fn command_available(command: &str) -> bool {
    match std::process::Command::new(command)
        .arg("--version")
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub(crate) fn posix_sh_available() -> bool {
    std::process::Command::new("sh")
        .args(["-c", ":"])
        .status()
        .is_ok_and(|status| status.success())
}
