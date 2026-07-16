#[cfg(feature = "repo-tests")]
use std::ffi::{OsStr, OsString};
#[cfg(feature = "system-tests")]
use std::fs;
#[cfg(feature = "repo-tests")]
use std::fs::DirEntry;
use std::fs::File;
use std::io;
use std::io::Read;
use std::path::Path;

#[cfg(feature = "repo-tests")]
pub fn filter_entry(entry: &DirEntry, filter: Option<&OsString>) -> bool {
    let path = entry.path();
    let name = match path.file_name().and_then(OsStr::to_str) {
        Some(name) => name,
        None => return false,
    };
    if name.starts_with('.') || name.ends_with('~') {
        return false;
    }
    if let Some(filter) = filter.and_then(|s| s.to_str()) {
        return name.starts_with(filter);
    }

    true
}

pub fn manifest_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{} = ", key);
    if !line.starts_with(&prefix) {
        return None;
    }

    let value = line[prefix.len()..].trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return None;
    }

    Some(&value[1..value.len() - 1])
}

pub fn manifest_list_values(line: &str, key: &str) -> Option<Vec<String>> {
    let prefix = format!("{} = ", key);
    let value = if line.starts_with(&prefix) {
        line[prefix.len()..].trim()
    } else {
        return None;
    };

    if value.len() < 2 || !value.starts_with('[') || !value.ends_with(']') {
        return None;
    }

    let inner = value[1..value.len() - 1].trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }

    let mut values = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.len() < 2 || !item.starts_with('"') || !item.ends_with('"') {
            return None;
        }
        values.push(item[1..item.len() - 1].to_owned());
    }

    Some(values)
}

pub fn manifest_bool(line: &str, key: &str) -> Option<bool> {
    let prefix = format!("{} = ", key);
    let value = if line.starts_with(&prefix) {
        line[prefix.len()..].trim()
    } else {
        return None;
    };

    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

pub fn read_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}

#[cfg(feature = "system-tests")]
pub fn collect_fixture_dirs(root: &Path, cases: &mut Vec<std::path::PathBuf>) {
    if root.join("fixture.toml").is_file() {
        cases.push(root.to_path_buf());
        return;
    }

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries {
        let entry = entry.expect("reading fixture directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_fixture_dirs(&path, cases);
        }
    }
}
