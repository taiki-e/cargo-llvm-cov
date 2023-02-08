#!/bin/bash
# shellcheck disable=SC2046
set -euo pipefail
IFS=$'\n\t'
cd "$(dirname "$0")"/..

# shellcheck disable=SC2154
trap 's=$?; echo >&2 "$0: Error on line "${LINENO}": ${BASH_COMMAND}"; exit ${s}' ERR

# USAGE:
#    ./tools/tidy.sh
#
# Note: This script requires the following tools:
# - shfmt
# - shellcheck
# - npm
# - jq and yq (if this repository uses bors)
# - rustup (if Rust code exists)
# - clang-format (if C/C++ code exists)
#
# This script is shared with other repositories, so there may also be
# checks for files not included in this repository, but they will be
# skipped if the corresponding files do not exist.

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

if [[ $# -gt 0 ]]; then
    cat <<EOF
USAGE:
    $0
EOF
    exit 1
fi

# Rust (if exists)
if [[ -n "$(git ls-files '*.rs')" ]]; then
    if type -P rustup &>/dev/null; then
        # `cargo fmt` cannot recognize files not included in the current workspace and modules
        # defined inside macros, so run rustfmt directly.
        # We need to use nightly rustfmt because we use the unstable formatting options of rustfmt.
        rustc_version=$(rustc -Vv | grep 'release: ' | sed 's/release: //')
        if [[ "${rustc_version}" == *"nightly"* ]] || [[ "${rustc_version}" == *"dev"* ]]; then
            rustup component add rustfmt &>/dev/null
            echo "+ rustfmt \$(git ls-files '*.rs')"
            rustfmt $(git ls-files '*.rs')
        else
            rustup component add rustfmt --toolchain nightly &>/dev/null
            echo "+ rustfmt +nightly \$(git ls-files '*.rs')"
            rustfmt +nightly $(git ls-files '*.rs')
        fi
        check_diff $(git ls-files '*.rs')
    else
        warn "'rustup' is not installed"
    fi
fi

# C/C++ (if exists)
if [[ -n "$(git ls-files '*.c')$(git ls-files '*.cpp')" ]]; then
    if [[ ! -e .clang-format ]]; then
        warn "could not fount .clang-format in the repository root"
    fi
    if type -P clang-format &>/dev/null; then
        echo "+ clang-format -i \$(git ls-files '*.c') \$(git ls-files '*.cpp')"
        clang-format -i $(git ls-files '*.c') $(git ls-files '*.cpp')
        check_diff $(git ls-files '*.c') $(git ls-files '*.cpp')
    else
        warn "'clang-format' is not installed"
    fi
fi

# YAML/JavaScript/JSON (if exists)
if [[ -n "$(git ls-files '*.yml')$(git ls-files '*.js')$(git ls-files '*.json')" ]]; then
    if type -P npm &>/dev/null; then
        echo "+ npx prettier -l -w \$(git ls-files '*.yml') \$(git ls-files '*.js') \$(git ls-files '*.json')"
        npx prettier -l -w $(git ls-files '*.yml') $(git ls-files '*.js') $(git ls-files '*.json')
        check_diff $(git ls-files '*.yml') $(git ls-files '*.js') $(git ls-files '*.json')
    else
        warn "'npm' is not installed"
    fi
    if [[ -e .github/workflows/ci.yml ]] && grep -q '# tidy:needs' .github/workflows/ci.yml && ! grep -Eq '# *needs: \[' .github/workflows/ci.yml; then
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
    echo "+ shfmt -l -w \$(git ls-files '*.sh')"
    shfmt -l -w $(git ls-files '*.sh')
    check_diff $(git ls-files '*.sh')
else
    warn "'shfmt' is not installed"
fi
if type -P shellcheck &>/dev/null; then
    echo "+ shellcheck \$(git ls-files '*.sh')"
    if ! shellcheck $(git ls-files '*.sh'); then
        should_fail=1
    fi
    if [[ -n "$(git ls-files '*Dockerfile')" ]]; then
        # SC2154 doesn't seem to work on dockerfile.
        echo "+ shellcheck -e SC2148,SC2154,SC2250 \$(git ls-files '*Dockerfile')"
        if ! shellcheck -e SC2148,SC2154,SC2250 $(git ls-files '*Dockerfile'); then
            should_fail=1
        fi
    fi
else
    warn "'shellcheck' is not installed"
fi

# Spell check (if config exists)
if [[ -f .cspell.json ]]; then
    if type -P npm &>/dev/null; then
        if [[ -n "$(git ls-files '*Cargo.toml')" ]]; then
            dependencies=''
            for manifest_path in $(git ls-files '*Cargo.toml'); do
                if [[ "${manifest_path}" != "Cargo.toml" ]] && ! grep -Eq '\[workspace\]' "${manifest_path}"; then
                    continue
                fi
                metadata=$(cargo metadata --format-version=1 --all-features --no-deps --manifest-path "${manifest_path}")
                for id in $(jq <<<"${metadata}" '.workspace_members[]'); do
                    dependencies+=$'\n'
                    dependencies+=$(jq <<<"${metadata}" ".packages[] | select(.id == ${id})" | jq -r '.dependencies[].name')
                done
            done
            words=''
            # shellcheck disable=SC2001
            for word in $(sed <<<"${dependencies}" 's/[0-9_-]/\n/g' | LC_ALL=C sort -f -u | (grep -E '.{4,}' || true)); do
                # Skip if the word is contained in other dictionaries.
                if ! npx cspell trace "${word}" 2>/dev/null | (grep -v -E '/(project-dictionary|rust-dependencies)\.txt' || true) | grep -Eq "^${word} \* [0-9A-Za-z_-]+\* "; then
                    words+=$'\n'
                    words+="${word}"
                fi
            done
        fi
        cat >.github/.cspell/rust-dependencies.txt <<EOF
// This file is @generated by $(basename "$0").
// It is not intended for manual editing.
EOF
        if [[ -n "${words:-}" ]]; then
            echo "${words}" >>.github/.cspell/rust-dependencies.txt
        fi
        check_diff .github/.cspell/rust-dependencies.txt
        if ! grep -Eq "^\.github/\.cspell/rust-dependencies.txt linguist-generated" .gitattributes; then
            echo "warning: you may want to mark .github/.cspell/rust-dependencies.txt linguist-generated"
        fi

        echo "+ npx cspell --no-progress \$(git ls-files)"
        npx cspell --no-progress $(git ls-files)

        for dictionary in .github/.cspell/*.txt; do
            if [[ "${dictionary}" == .github/.cspell/project-dictionary.txt ]]; then
                continue
            fi
            dup=$(sed '/^$/d' .github/.cspell/project-dictionary.txt "${dictionary}" | LC_ALL=C sort -f | uniq -d -i | (grep -v '//.*' || true))
            if [[ -n "${dup}" ]]; then
                warn "duplicated words in dictionaries; please remove the following words from .github/.cspell/project-dictionary.txt"
                echo "======================================="
                echo "${dup}"
                echo "======================================="
            fi
        done
    else
        warn "'npm' is not installed"
    fi
fi

if [[ -n "${should_fail:-}" ]]; then
    exit 1
fi
