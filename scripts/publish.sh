#!/bin/bash

# Automate the local side release step.
#
# Usage:
#    ./scripts/publish.sh <version> [--dry-run]
#
# Note:
# - This script does not intend to use with projects that have multiple public
#   packages with different version numbers in the workspace, like crossbeam.
# - This script requires parse-changelog <https://github.com/taiki-e/parse-changelog>

set -euo pipefail
IFS=$'\n\t'

# A list of paths to the crate to be published.
MEMBERS=(
  "."
)

function error {
  echo "error: $*" >&2
}

cd "$(cd "$(dirname "${0}")" && pwd)"/..

git diff --exit-code
git diff --exit-code --staged

# Parse arguments.
version="${1:?}"
tag="v${version}"
if [[ ! "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z_0-9\.-]+)?(\+[a-zA-Z_0-9\.-]+)?$ ]]; then
  error "invalid version format: ${version}"
  exit 1
fi
if [[ "${2:-}" == "--dry-run" ]]; then
  dryrun="--dry-run"
  shift
fi
if [[ -n "${2:-}" ]]; then
  error "invalid argument: ${2}"
  exit 1
fi

# Make sure that the version number of the workspace members matches the specified version.
for member in "${MEMBERS[@]}"; do
  if [[ ! -d "${member}" ]]; then
    error "not found workspace member ${member}"
    exit 1
  fi
  (
    cd "${member}"
    actual=$(cargo pkgid | sed 's/.*#//')
    if [[ "${actual}" != "${version}" ]] && [[ "${actual}" != *":${version}" ]]; then
      error "expected to release version ${version}, but ${member}/Cargo.toml contained ${actual}"
      exit 1
    fi
  )
done

# Make sure that a valid release note for this version exists.
# https://github.com/taiki-e/parse-changelog
echo "========== changes =========="
parse-changelog CHANGELOG.md "${version}"
echo "============================="

# Make sure the same release has not been created in the past.
if gh release view "${tag}" &>/dev/null; then
  error "tag '${tag}' has already been created and pushed"
  exit 1
fi
if git --no-pager tag | grep "$tag" &>/dev/null; then
  error "tag '${tag}' has already been created"
  exit 1
fi

# Create and push tag.
if [[ -n "${dryrun:-}" ]]; then
  echo "warning: skip creating a new tag '${tag}' due to dry run"
else
  echo "info: creating and pushing a new tag '${tag}'"
  git tag "${tag}"
  git push origin --tags
fi
