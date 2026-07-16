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

test -f "$package_dir/README.md"
test -f "$package_dir/RELEASE.md"
test -f "$package_dir/LICENSE-MIT"
test -f "$package_dir/LICENSE-APACHE"
grep -Fqx "name = \"${package_name}\"" "$package_dir/Cargo.toml.orig"
grep -Fqx "version = \"${version}\"" "$package_dir/Cargo.toml.orig"
grep -Fqx "name = \"${crate_name}\"" "$package_dir/Cargo.toml.orig"
grep -Fqx 'rust-version = "1.89"' "$package_dir/Cargo.toml.orig"
grep -Fqx 'license = "MIT OR Apache-2.0"' "$package_dir/Cargo.toml.orig"
grep -Fqx 'publish = false' "$package_dir/Cargo.toml.orig"
if grep -Eq '(^|[[:space:]])path[[:space:]]*=' "$package_dir/Cargo.toml.orig"; then
    echo "packaged manifest contains a path dependency" >&2
    exit 1
fi

cargo test --locked --lib --manifest-path "$package_dir/Cargo.toml"
cargo test --locked --doc --manifest-path "$package_dir/Cargo.toml"

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
use ${crate_name}::contract::{
    ID_ALGORITHM_VERSION, SOURCE_PACKAGE_SCHEMA_ID, SOURCE_PACKAGE_SCHEMA_VERSION,
};

pub fn packaged_identity() -> (&'static str, u32, u16) {
    (
        SOURCE_PACKAGE_SCHEMA_ID,
        SOURCE_PACKAGE_SCHEMA_VERSION,
        ID_ALGORITHM_VERSION,
    )
}

#[cfg(test)]
mod tests {
    use super::packaged_identity;

    #[test]
    fn consumes_the_packaged_contract_identity() {
        assert_eq!(
            packaged_identity(),
            ("follang.parc.source-package", 2, 1),
        );
    }
}
EOF
cargo test --manifest-path "$scratch/consumer/Cargo.toml"
