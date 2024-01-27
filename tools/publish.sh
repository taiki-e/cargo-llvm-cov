#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
set -eEuo pipefail
IFS=$'\n\t'
cd "$(dirname "$0")"/..

# shellcheck disable=SC2154
trap 's=$?; echo >&2 "$0: error on line "${LINENO}": ${BASH_COMMAND}"; exit ${s}' ERR

# Publish a new release.
#
# USAGE:
#    ./tools/publish.sh <VERSION>
#
# Note: This script requires the following tools:
# - parse-changelog <https://github.com/taiki-e/parse-changelog>

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
tag_prefix="v"
tag="${tag_prefix}${version}"
changelog="CHANGELOG.md"
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

# Make sure that the release was created from an allowed branch.
if ! git branch | grep -q '\* main$'; then
    bail "current branch is not 'main'"
fi

release_date=$(date -u '+%Y-%m-%d')
tags=$(git --no-pager tag | (grep -E "^${tag_prefix}[0-9]+" || true))
if [[ -n "${tags}" ]]; then
    # Make sure the same release does not exist in changelog.
    if grep -Eq "^## \\[${version//./\\.}\\]" "${changelog}"; then
        bail "release ${version} already exist in ${changelog}"
    fi
    if grep -Eq "^\\[${version//./\\.}\\]: " "${changelog}"; then
        bail "link to ${version} already exist in ${changelog}"
    fi
    # Update changelog.
    remote_url=$(grep -E '^\[Unreleased\]: https://' "${changelog}" | sed 's/^\[Unreleased\]: //; s/\.\.\.HEAD$//')
    prev_tag="${remote_url#*/compare/}"
    remote_url="${remote_url%/compare/*}"
    sed -i "s/^## \\[Unreleased\\]/## [Unreleased]\\n\\n## [${version}] - ${release_date}/" "${changelog}"
    sed -i "s#^\[Unreleased\]: https://.*#[Unreleased]: ${remote_url}/compare/${tag}...HEAD\\n[${version}]: ${remote_url}/compare/${prev_tag}...${tag}#" "${changelog}"
    if ! grep -Eq "^## \\[${version//./\\.}\\] - ${release_date}$" "${changelog}"; then
        bail "failed to update ${changelog}"
    fi
    if ! grep -Eq "^\\[${version//./\\.}\\]: " "${changelog}"; then
        bail "failed to update ${changelog}"
    fi
else
    # Make sure the release exists in changelog.
    if ! grep -Eq "^## \\[${version//./\\.}\\] - ${release_date}$" "${changelog}"; then
        bail "release ${version} does not exist in ${changelog} or has wrong release date"
    fi
    if ! grep -Eq "^\\[${version//./\\.}\\]: " "${changelog}"; then
        bail "link to ${version} does not exist in ${changelog}"
    fi
fi

# Make sure that a valid release note for this version exists.
# https://github.com/taiki-e/parse-changelog
changes=$(parse-changelog "${changelog}" "${version}")
if [[ -z "${changes}" ]]; then
    bail "changelog for ${version} has no body"
fi
echo "============== CHANGELOG =============="
echo "${changes}"
echo "======================================="

metadata=$(cargo metadata --format-version=1 --no-deps)
prev_version=''
docs=()
for readme in $(git ls-files '*README.md'); do
    docs+=("${readme}")
    lib="$(dirname "${readme}")/src/lib.rs"
    if [[ -f "${lib}" ]]; then
        docs+=("${lib}")
    fi
done
changed_paths=("${changelog}" "${docs[@]}")
for id in $(jq <<<"${metadata}" '.workspace_members[]'); do
    pkg=$(jq <<<"${metadata}" ".packages[] | select(.id == ${id})")
    publish=$(jq <<<"${pkg}" -r '.publish')
    # Publishing is unrestricted if null, and forbidden if an empty array.
    if [[ "${publish}" == "[]" ]]; then
        continue
    fi
    name=$(jq <<<"${pkg}" -r '.name')
    actual_version=$(jq <<<"${pkg}" -r '.version')
    if [[ -z "${prev_version:-}" ]]; then
        prev_version="${actual_version}"
    fi
    # Make sure that the version number of all publishable workspace members matches.
    if [[ "${actual_version}" != "${prev_version}" ]]; then
        bail "publishable workspace members must be version '${prev_version}', but package '${name}' is version '${actual_version}'"
    fi

    manifest_path=$(jq <<<"${pkg}" -r '.manifest_path')
    changed_paths+=("${manifest_path}")
    # Update version in Cargo.toml.
    sed -i -e "s/^version = \"${prev_version}\" #publish:version/version = \"${version}\" #publish:version/g" "${manifest_path}"
    # Update '=' requirement in Cargo.toml.
    for manifest in $(git ls-files '*Cargo.toml'); do
        if grep -Eq "^${name} = \\{ version = \"=${prev_version}\"" "${manifest}"; then
            sed -i -E -e "s/^${name} = \\{ version = \"=${prev_version}\"/${name} = { version = \"=${version}\"/g" "${manifest}"
        fi
    done
    # Update version in readme and lib.rs.
    for path in "${docs[@]}"; do
        # TODO: handle pre-release
        if [[ "${version}" == "0.0."* ]]; then
            # 0.0.x -> 0.0.x2
            if grep -Eq "^${name} = \"${prev_version}\"" "${path}"; then
                sed -i -E -e "s/^${name} = \"${prev_version}\"/${name} = \"${version}\"/g" "${path}"
            fi
            if grep -Eq "^${name} = \\{ version = \"${prev_version}\"" "${path}"; then
                sed -i -E -e "s/^${name} = \\{ version = \"${prev_version}\"/${name} = { version = \"${version}\"/g" "${path}"
            fi
        elif [[ "${version}" == "0."* ]]; then
            prev_major_minor="${prev_version%.*}"
            major_minor="${version%.*}"
            if [[ "${prev_major_minor}" != "${major_minor}" ]]; then
                # 0.x -> 0.x2
                # 0.x.* -> 0.x2
                if grep -Eq "^${name} = \"${prev_major_minor}(\\.[0-9]+)?\"" "${path}"; then
                    sed -i -E -e "s/^${name} = \"${prev_major_minor}(\\.[0-9]+)?\"/${name} = \"${major_minor}\"/g" "${path}"
                fi
                if grep -Eq "^${name} = \\{ version = \"${prev_major_minor}(\\.[0-9]+)?\"" "${path}"; then
                    sed -i -E -e "s/^${name} = \\{ version = \"${prev_major_minor}(\\.[0-9]+)?\"/${name} = { version = \"${major_minor}\"/g" "${path}"
                fi
            fi
        else
            prev_major="${prev_version%%.*}"
            major="${version%%.*}"
            if [[ "${prev_major}" != "${major}" ]]; then
                # x -> x2
                # x.* -> x2
                # x.*.* -> x2
                if grep -Eq "^${name} = \"${prev_major}(\\.[0-9]+(\\.[0-9]+)?)?\"" "${path}"; then
                    sed -i -E -e "s/^${name} = \"${prev_major}(\\.[0-9]+(\\.[0-9]+)?)?\"/${name} = \"${major}\"/g" "${path}"
                fi
                if grep -Eq "^${name} = \\{ version = \"${prev_major}(\\.[0-9]+(\\.[0-9]+)?)?\"" "${path}"; then
                    sed -i -E -e "s/^${name} = \\{ version = \"${prev_major}(\\.[0-9]+(\\.[0-9]+)?)?\"/${name} = { version = \"${major}\"/g" "${path}"
                fi
            fi
        fi
    done
done

if [[ -n "${tags}" ]]; then
    # Create a release commit.
    x git add "${changed_paths[@]}"
    x git commit -m "Release ${version}"
fi

x git tag "${tag}"
x git push origin main
x git push origin --tags
