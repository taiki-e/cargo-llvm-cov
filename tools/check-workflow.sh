#!/bin/bash
# shellcheck disable=SC2046
set -euo pipefail
IFS=$'\n\t'
cd "$(dirname "$0")"/..

# USAGE:
#    ./tools/check-workflow.sh
#
# Note: This script requires the following tools:
# - jq
# - yq

bail() {
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
        echo "::error::$*"
    else
        echo "error: $*" >&2
    fi
    exit 1
}

if [[ $# -gt 0 ]]; then
    cat <<EOF
USAGE:
    $0
EOF
    exit 1
fi

# shellcheck disable=SC2207
jobs_actual=($(yq '.jobs' .github/workflows/ci.yml | jq -r 'keys_unsorted[]'))
unset 'jobs_actual[${#jobs_actual[@]}-1]'
# shellcheck disable=SC2207
jobs_expected=($(yq -r '.jobs."ci-success".needs[]' .github/workflows/ci.yml))
if [[ "${jobs_actual[*]}" != "${jobs_expected[*]+"${jobs_expected[*]}"}" ]]; then
    printf -v jobs '%s, ' "${jobs_actual[@]}"
    sed -i "s/needs: \[.*\] # tidy:needs/needs: [${jobs%, }] # tidy:needs/" .github/workflows/ci.yml
    git --no-pager diff .github/workflows/ci.yml
    bail "please update 'needs' section in 'ci-success' job to '[${jobs%, }]'"
fi
