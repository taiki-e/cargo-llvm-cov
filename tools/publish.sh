#!/bin/bash
set -euo pipefail
IFS=$'\n\t'

# Publish a new release.
#
# USAGE:
#    ./tools/publish.sh
#
# NOTE:
# - This script requires parse-changelog <https://github.com/taiki-e/parse-changelog>

cd "$(cd "$(dirname "$0")" && pwd)"/..

bail() {
    echo >&2 "error: $*"
    exit 1
}

if [[ $# -gt 0 ]]; then
    bail "invalid argument '$1'"
fi

# Make sure that the version number of all publishable workspace members matches.
metadata="$(cargo metadata --format-version=1 --all-features --no-deps)"
for id in $(jq <<<"${metadata}" '.workspace_members[]'); do
    pkg="$(jq <<<"${metadata}" ".packages[] | select(.id == ${id})")"
    publish=$(jq <<<"${pkg}" -r '.publish')
    # Publishing is unrestricted if null, and forbidden if an empty array.
    if [[ "${publish}" == "[]" ]]; then
        continue
    fi
    actual_version=$(jq <<<"${pkg}" -r '.version')
    if [[ -z "${version:-}" ]]; then
        version="${actual_version}"
    fi
    if [[ "${actual_version}" != "${version}" ]]; then
        name=$(jq <<<"${pkg}" -r '.name')
        bail "publishable workspace members must be version '${version}', but package '${name}' is version '${actual_version}'"
    fi
done
tag="v${version}"

# Make sure there is no uncommitted change.
git diff --exit-code
git diff --exit-code --staged

# Make sure that a valid release note for this version exists.
# https://github.com/taiki-e/parse-changelog
echo "============== CHANGELOG =============="
parse-changelog CHANGELOG.md "${version}"
echo "======================================="

if ! grep <CHANGELOG.md -E "^\\[${version//./\\.}\\]: " >/dev/null; then
    bail "not found link to [${version}] in CHANGELOG.md"
fi

# Make sure the same release has not been created in the past.
if gh release view "${tag}" &>/dev/null; then
    bail "tag '${tag}' has already been created and pushed"
fi

set -x

git push origin main
git tag "${tag}"
git push origin --tags
