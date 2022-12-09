#!/bin/bash
set -euo pipefail
IFS=$'\n\t'
cd "$(dirname "$0")"/..

# shellcheck disable=SC2154
trap 's=$?; echo >&2 "$0: Error on line "${LINENO}": ${BASH_COMMAND}"; exit ${s}' ERR

# Publish a new release.
#
# USAGE:
#    ./tools/publish.sh <VERSION>
#
# Note: This script requires the following tools:
# - parse-changelog <https://github.com/taiki-e/parse-changelog>
# - cargo-workspaces <https://github.com/pksunkara/cargo-workspaces>

x() {
    local cmd="$1"
    shift
    (
        set -x
        "${cmd}" "$@"
    )
}
bail() {
    echo >&2 "error: $*"
    exit 1
}

version="${1:?}"
version="${version#v}"
tag="v${version}"
if [[ ! "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z\.-]+)?(\+[0-9A-Za-z\.-]+)?$ ]]; then
    bail "invalid version format '${version}'"
fi
if [[ $# -gt 1 ]]; then
    bail "invalid argument '$2'"
fi

# Make sure there is no uncommitted change.
git diff --exit-code
git diff --exit-code --staged

# Make sure the same release has not been created in the past.
if gh release view "${tag}" &>/dev/null; then
    bail "tag '${tag}' has already been created and pushed"
fi

if ! git branch | grep -q '\* main'; then
    bail "current branch is not 'main'"
fi

tags=$(git --no-pager tag)
if [[ -n "${tags}" ]]; then
    # Make sure the same release does not exist in CHANGELOG.md.
    release_date=$(date -u '+%Y-%m-%d')
    if grep -Eq "^## \\[${version//./\\.}\\] - ${release_date}$" CHANGELOG.md; then
        bail "release ${version} already exist in CHANGELOG.md"
    fi
    if grep -Eq "^\\[${version//./\\.}\\]: " CHANGELOG.md; then
        bail "link to ${version} already exist in CHANGELOG.md"
    fi

    # Update changelog.
    remote_url=$(grep -E '^\[Unreleased\]: https://' CHANGELOG.md | sed 's/^\[Unreleased\]: //' | sed 's/\.\.\.HEAD$//')
    before_tag=$(sed <<<"${remote_url}" 's/^.*\/compare\///')
    remote_url=$(sed <<<"${remote_url}" 's/\/compare\/v.*$//')
    sed -i "s/^## \\[Unreleased\\]/## [Unreleased]\\n\\n## [${version}] - ${release_date}/" CHANGELOG.md
    sed -i "s#^\[Unreleased\]: https://.*#[Unreleased]: ${remote_url}/compare/v${version}...HEAD\\n[${version}]: ${remote_url}/compare/${before_tag}...v${version}#" CHANGELOG.md
    if ! grep -Eq "^## \\[${version//./\\.}\\] - ${release_date}$" CHANGELOG.md; then
        bail "failed to update CHANGELOG.md"
    fi
    if ! grep -Eq "^\\[${version//./\\.}\\]: " CHANGELOG.md; then
        bail "failed to update CHANGELOG.md"
    fi
fi

# Make sure that a valid release note for this version exists.
# https://github.com/taiki-e/parse-changelog
echo "============== CHANGELOG =============="
parse-changelog CHANGELOG.md "${version}"
echo "======================================="

metadata="$(cargo metadata --format-version=1 --all-features --no-deps)"
prev_version=''
manifest_paths=()
for id in $(jq <<<"${metadata}" '.workspace_members[]'); do
    pkg="$(jq <<<"${metadata}" ".packages[] | select(.id == ${id})")"
    publish=$(jq <<<"${pkg}" -r '.publish')
    # Publishing is unrestricted if null, and forbidden if an empty array.
    if [[ "${publish}" == "[]" ]]; then
        continue
    fi
    actual_version=$(jq <<<"${pkg}" -r '.version')
    if [[ -z "${prev_version:-}" ]]; then
        prev_version="${actual_version}"
    fi
    # Make sure that the version number of all publishable workspace members matches.
    if [[ "${actual_version}" != "${prev_version}" ]]; then
        name=$(jq <<<"${pkg}" -r '.name')
        bail "publishable workspace members must be version '${prev_version}', but package '${name}' is version '${actual_version}'"
    fi

    manifest_path=$(jq <<<"${pkg}" -r '.manifest_path')
    manifest_paths+=("${manifest_path}")
done

# Update version.
x cargo workspaces version --force '*' --no-git-commit --exact -y custom "${version}"

if [[ -n "${tags}" ]]; then
    # Create a release commit.
    x git add CHANGELOG.md "${manifest_paths[@]}"
    x git commit -m "Release ${version}"
fi

x git tag "${tag}"
x git push origin main
x git push origin --tags
