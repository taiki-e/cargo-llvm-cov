#!/bin/bash
# shellcheck disable=SC2046
set -euo pipefail
IFS=$'\n\t'
cd "$(dirname "$0")"/..

# USAGE:
#    ./tools/tidy.sh
#
# Note: This script requires the following tools:
# - shfmt
# - shellcheck
# - npm (if any of YAML/JavaScript/JSON exists)
# - jq and yq (if this repository uses bors)
# - clang-format (if any of C/C++ exists)
#
# This script is shared with other repositories, so there may also be
# checks for files not included in this repository, but they will be
# skipped if the corresponding files do not exist.

x() {
    local cmd="$1"
    shift
    if [[ -n "${verbose:-}" ]]; then
        (
            set -x
            "${cmd}" "$@"
        )
    else
        "${cmd}" "$@"
    fi
}
check_diff() {
    if [[ -n "${CI:-}" ]]; then
        if ! git --no-pager diff --exit-code "$@"; then
            should_fail=1
        fi
    else
        if ! git --no-pager diff --exit-code "$@" &>/dev/null; then
            should_fail=1
        fi
    fi
}
warn() {
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
        echo "::warning::$*"
    else
        echo >&2 "warning: $*"
    fi
    should_fail=1
}

if [[ "${1:-}" == "-v" ]]; then
    shift
    verbose=1
fi
if [[ -n "${CI:-}" ]]; then
    verbose=1
fi
if [[ $# -gt 0 ]]; then
    cat <<EOF
USAGE:
    $0 [-v]
EOF
    exit 1
fi

# C/C++ (if exists)
if [[ -n "$(git ls-files '*.c')$(git ls-files '*.cpp')" ]]; then
    if [[ ! -e .clang-format ]]; then
        warn "could not fount .clang-format in the repository root"
    fi
    if type -P clang-format &>/dev/null; then
        x clang-format -i $(git ls-files '*.c') $(git ls-files '*.cpp')
        check_diff $(git ls-files '*.c') $(git ls-files '*.cpp')
    else
        warn "'clang-format' is not installed"
    fi
fi

# YAML/JavaScript/JSON (if exists)
if [[ -n "$(git ls-files '*.yml')$(git ls-files '*.js')$(git ls-files '*.json')" ]]; then
    if type -P npm &>/dev/null; then
        if [[ ! -e node_modules/.bin/prettier ]]; then
            x npm install prettier &>/dev/null
        fi
        x npx prettier -l -w $(git ls-files '*.yml') $(git ls-files '*.js') $(git ls-files '*.json')
        check_diff $(git ls-files '*.yml') $(git ls-files '*.js') $(git ls-files '*.json')
    else
        warn "'npm' is not installed"
    fi
    if [[ -e .github/workflows/ci.yml ]] && grep -q '# tidy:needs' .github/workflows/ci.yml; then
        if type -P jq &>/dev/null && type -P yq &>/dev/null; then
            # shellcheck disable=SC2207
            jobs_actual=($(yq '.jobs' .github/workflows/ci.yml | jq -r 'keys_unsorted[]'))
            unset 'jobs_actual[${#jobs_actual[@]}-1]'
            # shellcheck disable=SC2207
            jobs_expected=($(yq -r '.jobs."ci-success".needs[]' .github/workflows/ci.yml))
            if [[ "${jobs_actual[*]}" != "${jobs_expected[*]+"${jobs_expected[*]}"}" ]]; then
                printf -v jobs '%s, ' "${jobs_actual[@]}"
                sed -i "s/needs: \[.*\] # tidy:needs/needs: [${jobs%, }] # tidy:needs/" .github/workflows/ci.yml
                check_diff .github/workflows/ci.yml
                warn "please update 'needs' section in 'ci-success' job"
            fi
        else
            warn "'jq' or 'yq' is not installed"
        fi
    fi
fi
if [[ -n "$(git ls-files '*.yaml')" ]]; then
    warn "please use '.yml' instead of '.yaml' for consistency"
    git ls-files '*.yaml'
fi

# Shell scripts
if type -P shfmt &>/dev/null; then
    x shfmt -l -w $(git ls-files '*.sh')
    check_diff $(git ls-files '*.sh')
else
    warn "'shfmt' is not installed"
fi
if type -P shellcheck &>/dev/null; then
    if ! x shellcheck $(git ls-files '*.sh'); then
        should_fail=1
    fi
    if [[ -n "$(git ls-files '*Dockerfile')" ]]; then
        # SC2154 doesn't seem to work on dockerfile.
        if ! x shellcheck -e SC2148,SC2154,SC2250 $(git ls-files '*Dockerfile'); then
            should_fail=1
        fi
    fi
else
    warn "'shellcheck' is not installed"
fi

if [[ -n "${should_fail:-}" ]]; then
    exit 1
fi
