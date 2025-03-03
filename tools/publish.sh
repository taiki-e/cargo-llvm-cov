#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
set -CeEuo pipefail
IFS=$'\n\t'
trap -- 's=$?; printf >&2 "%s\n" "${0##*/}:${LINENO}: \`${BASH_COMMAND}\` exit with ${s}"; exit ${s}' ERR
cd -- "$(dirname -- "$0")"/..

# Publish a new release.
#
# USAGE:
#    ./tools/publish.sh <VERSION>
#
# Note: This script requires the following tools:
# - parse-changelog <https://github.com/taiki-e/parse-changelog>

retry() {
  for i in {1..10}; do
    if "$@"; then
      return 0
    else
      sleep "${i}"
    fi
  done
  "$@"
}
bail() {
  printf >&2 'error: %s\n' "$*"
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
if { sed --help 2>&1 || true; } | grep -Eq -e '-i extension'; then
  in_place=(-i '')
else
  in_place=(-i)
fi

# Make sure there is no uncommitted change.
git diff --exit-code
git diff --exit-code --staged

# Make sure the same release has not been created in the past.
if gh release view "${tag}" &>/dev/null; then
  bail "tag '${tag}' has already been created and pushed"
fi

# Make sure that the release was created from an allowed branch.
if ! git branch | grep -Eq '\* main$'; then
  bail "current branch is not 'main'"
fi
if ! git remote -v | grep -F origin | grep -Eq 'github\.com[:/]taiki-e/'; then
  bail "cannot publish a new release from fork repository"
fi

release_date=$(date -u '+%Y-%m-%d')
tags=$(git --no-pager tag | { grep -E "^${tag_prefix}[0-9]+" || true; })
if [[ -n "${tags}" ]]; then
  # Make sure the same release does not exist in changelog.
  if grep -Eq "^## \\[${version//./\\.}\\]" "${changelog}"; then
    bail "release ${version} already exist in ${changelog}"
  fi
  if grep -Eq "^\\[${version//./\\.}\\]: " "${changelog}"; then
    bail "link to ${version} already exist in ${changelog}"
  fi
  # Update changelog.
  remote_url=$(grep -E '^\[Unreleased\]: https://' "${changelog}" | sed -E 's/^\[Unreleased\]: //; s/\.\.\.HEAD$//')
  prev_tag="${remote_url#*/compare/}"
  remote_url="${remote_url%/compare/*}"
  sed -E "${in_place[@]}" \
    -e "s/^## \\[Unreleased\\]/## [Unreleased]\\n\\n## [${version}] - ${release_date}/" \
    -e "s#^\[Unreleased\]: https://.*#[Unreleased]: ${remote_url}/compare/${tag}...HEAD\\n[${version}]: ${remote_url}/compare/${prev_tag}...${tag}#" "${changelog}"
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
printf '============== CHANGELOG ==============\n'
printf '%s\n' "${changes}"
printf '=======================================\n'

metadata=$(cargo metadata --format-version=1 --no-deps)
prev_version=''
docs=()
for readme in $(git ls-files '*README.md'); do
  docs+=("${readme}")
  lib="$(dirname -- "${readme}")/src/lib.rs"
  if [[ -f "${lib}" ]]; then
    docs+=("${lib}")
  fi
done
changed_paths=("${changelog}" "${docs[@]}")
# Publishing is unrestricted if null, and forbidden if an empty array.
for pkg in $(jq -c '. as $metadata | .workspace_members[] as $id | $metadata.packages[] | select(.id == $id and .publish != [])' <<<"${metadata}"); do
  eval "$(jq -r '@sh "NAME=\(.name) ACTUAL_VERSION=\(.version) manifest_path=\(.manifest_path)"' <<<"${pkg}")"
  if [[ -z "${prev_version}" ]]; then
    prev_version="${ACTUAL_VERSION}"
  fi
  # Make sure that the version number of all publishable workspace members matches.
  if [[ "${ACTUAL_VERSION}" != "${prev_version}" ]]; then
    bail "publishable workspace members must be version '${prev_version}', but package '${NAME}' is version '${ACTUAL_VERSION}'"
  fi

  changed_paths+=("${manifest_path}")
  # Update version in Cargo.toml.
  if ! grep -Eq "^version = \"${prev_version}\" #publish:version" "${manifest_path}"; then
    bail "not found '#publish:version' in version in ${manifest_path}"
  fi
  sed -E "${in_place[@]}" "s/^version = \"${prev_version}\" #publish:version/version = \"${version}\" #publish:version/g" "${manifest_path}"
  # Update '=' requirement in Cargo.toml.
  for manifest in $(git ls-files '*Cargo.toml'); do
    if grep -Eq "^${NAME} = \\{ version = \"=${prev_version}\"" "${manifest}"; then
      sed -E "${in_place[@]}" "s/^${NAME} = \\{ version = \"=${prev_version}\"/${NAME} = { version = \"=${version}\"/g" "${manifest}"
    fi
  done
  # Update version in readme and lib.rs.
  for path in "${docs[@]}"; do
    # TODO: handle pre-release
    if [[ "${version}" == "0.0."* ]]; then
      # 0.0.x -> 0.0.y
      if grep -Eq "^${NAME} = \"${prev_version}\"" "${path}"; then
        sed -E "${in_place[@]}" "s/^${NAME} = \"${prev_version}\"/${NAME} = \"${version}\"/g" "${path}"
      fi
      if grep -Eq "^${NAME} = \\{ version = \"${prev_version}\"" "${path}"; then
        sed -E "${in_place[@]}" "s/^${NAME} = \\{ version = \"${prev_version}\"/${NAME} = { version = \"${version}\"/g" "${path}"
      fi
    elif [[ "${version}" == "0."* ]]; then
      prev_major_minor="${prev_version%.*}"
      major_minor="${version%.*}"
      if [[ "${prev_major_minor}" != "${major_minor}" ]]; then
        # 0.x -> 0.y
        # 0.x.* -> 0.y
        if grep -Eq "^${NAME} = \"${prev_major_minor}(\\.[0-9]+)?\"" "${path}"; then
          sed -E "${in_place[@]}" "s/^${NAME} = \"${prev_major_minor}(\\.[0-9]+)?\"/${NAME} = \"${major_minor}\"/g" "${path}"
        fi
        if grep -Eq "^${NAME} = \\{ version = \"${prev_major_minor}(\\.[0-9]+)?\"" "${path}"; then
          sed -E "${in_place[@]}" "s/^${NAME} = \\{ version = \"${prev_major_minor}(\\.[0-9]+)?\"/${NAME} = { version = \"${major_minor}\"/g" "${path}"
        fi
      fi
    else
      prev_major="${prev_version%%.*}"
      major="${version%%.*}"
      if [[ "${prev_major}" != "${major}" ]]; then
        # x -> y
        # x.* -> y
        # x.*.* -> y
        if grep -Eq "^${NAME} = \"${prev_major}(\\.[0-9]+(\\.[0-9]+)?)?\"" "${path}"; then
          sed -E "${in_place[@]}" "s/^${NAME} = \"${prev_major}(\\.[0-9]+(\\.[0-9]+)?)?\"/${NAME} = \"${major}\"/g" "${path}"
        fi
        if grep -Eq "^${NAME} = \\{ version = \"${prev_major}(\\.[0-9]+(\\.[0-9]+)?)?\"" "${path}"; then
          sed -E "${in_place[@]}" "s/^${NAME} = \\{ version = \"${prev_major}(\\.[0-9]+(\\.[0-9]+)?)?\"/${NAME} = { version = \"${major}\"/g" "${path}"
        fi
      fi
    fi
  done
done

if [[ -n "${tags}" ]]; then
  # Create a release commit.
  (
    set -x
    git add "${changed_paths[@]}"
    git commit -m "Release ${version}"
  )
fi

set -x

git tag "${tag}"
retry git push origin refs/heads/main
retry git push origin refs/tags/"${tag}"
