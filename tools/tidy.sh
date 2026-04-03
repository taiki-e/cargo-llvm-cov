#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
set -CeEuo pipefail
IFS=$'\n\t'
trap -- 'printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
cd -- "$(dirname -- "$0")"/..

# USAGE:
#    GITHUB_TOKEN=$(gh auth token) ./tools/tidy.sh
#
# Note: This script requires the following tools:
# - docker
#
# This script is shared by projects under github.com/taiki-e, so there may also
# be checks for files not included in this repository, but they will be skipped
# if the corresponding files do not exist.
# It is not intended for manual editing.

if [[ $# -gt 0 ]]; then
  cat <<EOF
USAGE:
    $0
EOF
  exit 1
fi

if [[ -n "${TIDY_DEV:-}" ]]; then
  image="ghcr.io/taiki-e/tidy:latest"
else
  image="ghcr.io/taiki-e/tidy@sha256:4552cbce9426e102f9650cd9f8381e836fc8fda081dcbddcc7f31b15d48d1654"
fi
user="$(id -u):$(id -g)"
workdir=$(pwd)
tmp=$(mktemp -d)
trap -- 'rm -rf -- "${tmp:?}"' EXIT
mkdir -p -- "${tmp}/zizmor"
touch -- "${tmp}/dummy"
mkdir -- "${tmp}/dummy-dir"
code=0
color=''
if [[ -t 1 ]] || [[ -n "${GITHUB_ACTIONS:-}" ]]; then
  color=1
fi
common_args=(
  run --rm --init -i --user "${user}"
  --cap-drop=all
  --security-opt=no-new-privileges
  --read-only
  --env GITHUB_ACTIONS
  --env CI
  --env CARGO_TERM_COLOR
  --env REMOVE_UNUSED_WORDS
  --env TIDY_COLOR_ALWAYS="${color}"
  --env TIDY_CALLER="$0"
  --env TIDY_EXPECTED_MARKDOWN_FILE_COUNT
  --env TIDY_EXPECTED_RUST_FILE_COUNT
  --env TIDY_EXPECTED_CLANG_FORMAT_FILE_COUNT
  --env TIDY_EXPECTED_PRETTIER_FILE_COUNT
  --env TIDY_EXPECTED_TOML_FILE_COUNT
  --env TIDY_EXPECTED_SHELL_FILE_COUNT
  --env TIDY_EXPECTED_DOCKER_FILE_COUNT
)
# Map ignored files (e.g., .env) to dummy files.
while IFS= read -r path; do
  if [[ -d "${path}" ]]; then
    common_args+=(
      --mount "type=bind,source=${tmp}/dummy-dir,target=${workdir}/${path},readonly"
    )
  else
    common_args+=(
      --mount "type=bind,source=${tmp}/dummy,target=${workdir}/${path},readonly"
    )
  fi
done < <(git status --porcelain --ignored | grep -E '^!!' | cut -d' ' -f2)

docker_run() {
  docker "${common_args[@]}" "$@"
  code2="$?"
  if [[ ${code} -eq 0 ]] && [[ ${code2} -ne 0 ]]; then
    code="${code2}"
  fi
}

set +e
docker_run \
  --mount "type=bind,source=${workdir},target=${workdir}" --workdir "${workdir}" \
  --network=none \
  "${image}" \
  /checks/offline.sh
# Some good audits requires access to GitHub API.
docker_run \
  --mount "type=bind,source=${workdir},target=${workdir},readonly" --workdir "${workdir}" \
  --mount "type=bind,source=${tmp}/zizmor,target=/.cache/zizmor" \
  --env GH_TOKEN --env GITHUB_TOKEN --env ZIZMOR_GITHUB_TOKEN \
  "${image}" \
  /checks/zizmor.sh
# We use remote dictionary.
docker_run \
  --mount "type=bind,source=${workdir},target=${workdir},readonly" --workdir "${workdir}" \
  --mount "type=bind,source=${workdir}/.cspell.json,target=${workdir}/.cspell.json" \
  --mount "type=bind,source=${workdir}/.github/.cspell/project-dictionary.txt,target=${workdir}/.github/.cspell/project-dictionary.txt" \
  --mount "type=bind,source=${workdir}/.github/.cspell/rust-dependencies.txt,target=${workdir}/.github/.cspell/rust-dependencies.txt" \
  "${image}" \
  /checks/cspell.sh

exit "${code}"
