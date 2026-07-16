#!/usr/bin/env bash
set -euo pipefail

package_name=${1:?package name is required}
crate_name=${2:?crate name is required}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml" | head -n 1)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/${crate_name}-package.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

cd "$root"
archive="$root/target/package/${package_name}-${version}.crate"
rm -f "$archive"
cargo package --allow-dirty --no-verify
test -f "$archive"
tar -xzf "$archive" -C "$scratch"
package_dir="$scratch/${package_name}-${version}"

cargo test --manifest-path "$package_dir/Cargo.toml"

mkdir -p "$scratch/consumer/src"
cat >"$scratch/consumer/Cargo.toml" <<EOF
[package]
name = "${crate_name}-package-consumer"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
${crate_name} = { package = "${package_name}", path = "${package_dir}" }
EOF
cat >"$scratch/consumer/src/lib.rs" <<EOF
use ${crate_name} as _;

pub fn packaged_dependency_compiles() -> bool {
    true
}
EOF
cargo check --manifest-path "$scratch/consumer/Cargo.toml"
