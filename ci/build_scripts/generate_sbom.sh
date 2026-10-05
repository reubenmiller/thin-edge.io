#!/usr/bin/env bash
set -e

help() {
  cat <<EOF
Generate the Software Bill Of Materials (sbom), one per released binary

The sbom files are written in the CycloneDX json format, and their component version
is set to the git-derived version (the one used by the released binaries and packages)
rather than the version from Cargo.toml.

Usage:
    $0 [OUTPUT_DIR]

Args:
    OUTPUT_DIR  Directory where the sbom files are written to. Defaults to target/sbom

Examples:
    $0
    # Generate the sbom files under target/sbom
EOF
}

case "$1" in
    -h|--help)
        help
        exit 0
        ;;
esac

# Run from the project root so that the script can be called from anywhere
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.."

OUTPUT_DIR="${1:-target/sbom}"

# Note: keep the version in sync with the sbom job of .github/workflows/build-workflow.yml
# so that a locally generated sbom matches the released one
CYCLONEDX_VERSION="0.5.9"
if ! cargo cyclonedx --version 2>/dev/null | grep -qw "$CYCLONEDX_VERSION"; then
    cargo install --locked --version "$CYCLONEDX_VERSION" cargo-cyclonedx
fi

VERSION=$(./ci/build_scripts/version.sh)
GIT_COMMIT=$(git rev-parse HEAD)

# Remove any sbom left behind by a previous (interrupted) run so that a stale sbom
# is never collected below
find crates plugins -name '*.cdx.json' -delete

# Note: '--target all' is used as the binaries are released for several platforms
# (e.g. musl, darwin) whose dependencies differ. Without it the sbom would only
# describe the host platform and silently omit, say, the macOS specific dependencies.
#
# Note: cargo-cyclonedx describes the resolved dependency graph rather than the graph
# which is actually built, so the sbom is a superset of what is linked into the
# binaries. This is not affected by the feature flags: the output is identical with
# and without --all-features.
cargo cyclonedx --describe binaries --all-features --all --no-build-deps --format json --target all -v

# cargo-cyclonedx emits an sbom for every crate with a binary target, writes it next
# to that crate and suffixes it with '_bin', so keep only the sboms of the released
# binaries and collect them under their binary name to make publishing them easier.
# Note: `tedge` is a multicall binary, so its sbom already covers the components
# which are not built as a standalone binary (e.g. tedge-mapper, tedge-write)
# shellcheck disable=SC1091
source ./ci/package_list.sh

mkdir -p "$OUTPUT_DIR"
rm -f "$OUTPUT_DIR"/*.cdx.json
for name in "${BINARIES[@]}"; do
    sbom=$(find crates plugins -name "${name}_bin.cdx.json")
    if [ -z "$sbom" ]; then
        echo "No sbom was generated for the '$name' binary" >&2
        exit 1
    fi

    # Use the git-derived version (as used by the released binaries/packages) rather than
    # the Cargo.toml version, and record the exact commit the sbom was generated from
    jq --arg version "$VERSION" --arg commit "$GIT_COMMIT" '
        .metadata.component.version = $version
        | .metadata.properties += [{"name": "tedge:git:commit", "value": $commit}]
    ' "$sbom" > "$OUTPUT_DIR/${name}.cdx.json"
    rm -f "$sbom"
done

# Discard the sboms of the components which are covered by a multicall binary
find crates plugins -name '*.cdx.json' -delete

ls -l "$OUTPUT_DIR"
