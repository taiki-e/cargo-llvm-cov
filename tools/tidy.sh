#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# shellcheck disable=SC2046
set -eEuo pipefail
IFS=$'\n\t'
cd "$(dirname "$0")"/..

# shellcheck disable=SC2154
trap 's=$?; echo >&2 "$0: error on line "${LINENO}": ${BASH_COMMAND}"; exit ${s}' ERR

# USAGE:
#    ./tools/tidy.sh
#
# Note: This script requires the following tools:
# - shfmt
# - shellcheck
# - npm
# - jq and yq
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
info() {
    echo >&2 "info: $*"
}
warn() {
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
        echo "::warning::$*"
    else
        echo >&2 "warning: $*"
    fi
    should_fail=1
}
error() {
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
        echo "::error::$*"
    else
        echo >&2 "error: $*"
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
    info "checking Rust code style"
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
        warn "'rustup' is not installed; skipped Rust code style check"
    fi
    cast_without_turbofish=$(grep -n -E '\.cast\(\)' $(git ls-files '*.rs') || true)
    if [[ -n "${cast_without_turbofish}" ]]; then
        error "please replace \`.cast()\` with \`.cast::<type_name>()\`:"
        echo "${cast_without_turbofish}"
    fi
    first='1'
    for readme in $(git ls-files '*README.md'); do
        if ! grep -q '^<!-- tidy:crate-doc:start -->' "${readme}"; then
            continue
        fi
        lib="$(dirname "${readme}")/src/lib.rs"
        if [[ -n "${first}" ]]; then
            first=''
            info "checking readme and crate-level doc are synchronized"
        fi
        if ! grep -q '^<!-- tidy:crate-doc:end -->' "${readme}"; then
            bail "missing '<!-- tidy:crate-doc:end -->' comment in ${readme}"
        fi
        if ! grep -q '^<!-- tidy:crate-doc:start -->' "${lib}"; then
            bail "missing '<!-- tidy:crate-doc:start -->' comment in ${lib}"
        fi
        if ! grep -q '^<!-- tidy:crate-doc:end -->' "${lib}"; then
            bail "missing '<!-- tidy:crate-doc:end -->' comment in ${lib}"
        fi
        new=$(tr <"${readme}" '\n' '\a' | grep -o '<!-- tidy:crate-doc:start -->.*<!-- tidy:crate-doc:end -->' | sed 's/\&/\\\&/g; s/\\/\\\\/g')
        new=$(tr <"${lib}" '\n' '\a' | awk -v new="${new}" 'gsub("<!-- tidy:crate-doc:start -->.*<!-- tidy:crate-doc:end -->",new)' | tr '\a' '\n')
        echo "${new}" >"${lib}"
        check_diff "${lib}"
    done
    # Make sure that public Rust crates don't contain executables.
    failed_files=''
    metadata=$(cargo metadata --format-version=1 --no-deps)
    has_public_crate=''
    for id in $(jq <<<"${metadata}" '.workspace_members[]'); do
        pkg=$(jq <<<"${metadata}" ".packages[] | select(.id == ${id})")
        publish=$(jq <<<"${pkg}" -r '.publish')
        # Publishing is unrestricted if null, and forbidden if an empty array.
        if [[ "${publish}" == "[]" ]]; then
            continue
        fi
        has_public_crate='1'
    done
    if [[ -n "${has_public_crate}" ]]; then
        info "checking file permissions"
        if [[ -f Cargo.toml ]] && grep -Eq '^\[package\]' Cargo.toml && ! grep -Eq '^publish = false' Cargo.toml; then
            if ! grep -Eq '^exclude = \[.*\.\*.*\]' Cargo.toml; then
                error "top-level Cargo.toml of real manifest should have exclude field with \"/.*\" and \"/tools\""
            elif ! grep -Eq '^exclude = \[.*/tools.*\]' Cargo.toml; then
                error "top-level Cargo.toml of real manifest should have exclude field with \"/.*\" and \"/tools\""
            fi
        fi
        for p in $(git ls-files); do
            # Skip directories.
            if [[ -d "${p}" ]]; then
                continue
            fi
            # Top-level hidden files/directories and tools/* are excluded from crates.io (ensured by the above check).
            # TODO: fully respect exclude field in Cargo.toml.
            case "${p}" in
                .* | tools/*) continue ;;
            esac
            if [[ -x "${p}" ]]; then
                failed_files+="${p}"$'\n'
            fi
        done
        if [[ -n "${failed_files}" ]]; then
            error "file-permissions-check failed: executable should be in tools/ directory"
            echo "======================================="
            echo -n "${failed_files}"
            echo "======================================="
        fi
    fi
fi

# C/C++ (if exists)
if [[ -n "$(git ls-files '*.c')$(git ls-files '*.cpp')" ]]; then
    info "checking C/C++ code style"
    if [[ ! -e .clang-format ]]; then
        warn "could not fount .clang-format in the repository root"
    fi
    if type -P clang-format &>/dev/null; then
        echo "+ clang-format -i \$(git ls-files '*.c') \$(git ls-files '*.cpp')"
        clang-format -i $(git ls-files '*.c') $(git ls-files '*.cpp')
        check_diff $(git ls-files '*.c') $(git ls-files '*.cpp')
    else
        warn "'clang-format' is not installed; skipped C/C++ code style check"
    fi
fi

# YAML/JavaScript/JSON (if exists)
if [[ -n "$(git ls-files '*.yml')$(git ls-files '*.js')$(git ls-files '*.json')" ]]; then
    info "checking YAML/JavaScript/JSON code style"
    if type -P npm &>/dev/null; then
        echo "+ npx -y prettier -l -w \$(git ls-files '*.yml') \$(git ls-files '*.js') \$(git ls-files '*.json')"
        npx -y prettier -l -w $(git ls-files '*.yml') $(git ls-files '*.js') $(git ls-files '*.json')
        check_diff $(git ls-files '*.yml') $(git ls-files '*.js') $(git ls-files '*.json')
    else
        warn "'npm' is not installed; skipped YAML/JavaScript/JSON code style check"
    fi
    # Check GitHub workflows.
    if [[ -d .github/workflows ]]; then
        info "checking GitHub workflows"
        if type -P jq &>/dev/null && type -P yq &>/dev/null; then
            for workflow in .github/workflows/*.yml; do
                # The top-level permissions must be weak as they are referenced by all jobs.
                permissions=$(yq '.permissions' "${workflow}" | jq -c)
                case "${permissions}" in
                    '{"contents":"read"}' | '{"contents":"none"}') ;;
                    null) error "${workflow}: top level permissions not found; it must be 'contents: read' or weaker permissions" ;;
                    *) error "${workflow}: only 'contents: read' and weaker permissions are allowed at top level; if you want to use stronger permissions, please set job-level permissions" ;;
                esac
                # Make sure the 'needs' section is not out of date.
                if grep -q '# tidy:needs' "${workflow}" && ! grep -Eq '# *needs: \[' "${workflow}"; then
                    # shellcheck disable=SC2207
                    jobs_actual=($(yq '.jobs' "${workflow}" | jq -r 'keys_unsorted[]'))
                    unset 'jobs_actual[${#jobs_actual[@]}-1]'
                    # shellcheck disable=SC2207
                    jobs_expected=($(yq -r '.jobs."ci-success".needs[]' "${workflow}"))
                    if [[ "${jobs_actual[*]}" != "${jobs_expected[*]+"${jobs_expected[*]}"}" ]]; then
                        printf -v jobs '%s, ' "${jobs_actual[@]}"
                        sed -i "s/needs: \[.*\] # tidy:needs/needs: [${jobs%, }] # tidy:needs/" "${workflow}"
                        check_diff "${workflow}"
                        error "${workflow}: please update 'needs' section in 'ci-success' job"
                    fi
                fi
            done
        else
            warn "'jq' or 'yq' is not installed; skipped GitHub workflow check"
        fi
    fi
fi
if [[ -n "$(git ls-files '*.yaml')" ]]; then
    error "please use '.yml' instead of '.yaml' for consistency"
    git ls-files '*.yaml'
fi

# Markdown (if exists)
if [[ -n "$(git ls-files '*.md')" ]]; then
    info "checking Markdown style"
    if type -P npm &>/dev/null; then
        echo "+ npx -y markdownlint-cli2 \$(git ls-files '*.md')"
        npx -y markdownlint-cli2 $(git ls-files '*.md')
    else
        warn "'npm' is not installed; skipped Markdown style check"
    fi
fi
if [[ -n "$(git ls-files '*.markdown')" ]]; then
    error "please use '.md' instead of '.markdown' for consistency"
    git ls-files '*.markdown'
fi

# Shell scripts
info "checking Shell scripts"
if type -P shfmt &>/dev/null; then
    echo "+ shfmt -l -w \$(git ls-files '*.sh')"
    shfmt -l -w $(git ls-files '*.sh')
    check_diff $(git ls-files '*.sh')
else
    warn "'shfmt' is not installed; skipped Shell scripts style check"
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
    warn "'shellcheck' is not installed; skipped Shell scripts style check"
fi

# License check
# TODO: This check is still experimental and does not track all files that should be tracked.
if [[ -f tools/.tidy-check-license-headers ]]; then
    info "checking license headers (experimental)"
    failed_files=''
    for p in $(eval $(<tools/.tidy-check-license-headers)); do
        case "$(basename "${p}")" in
            *.stderr | *.expanded.rs) continue ;; # generated files
            *.sh | *.py | *.rb | *Dockerfile) prefix=("# ") ;;
            *.rs | *.c | *.h | *.cpp | *.hpp | *.s | *.S | *.js) prefix=("// " "/* ") ;;
            *.ld | *.x) prefix=("/* ") ;;
            # TODO: More file types?
            *) continue ;;
        esac
        # TODO: The exact line number is not actually important; it is important
        # that it be part of the top-level comments of the file.
        line="1"
        if IFS= LC_ALL=C read -rn3 -d '' shebang <"${p}" && [[ "${shebang}" == '#!/' ]]; then
            line="2"
        elif [[ "${p}" == *"Dockerfile" ]] && IFS= LC_ALL=C read -rn9 -d '' syntax <"${p}" && [[ "${syntax}" == '# syntax=' ]]; then
            line="2"
        fi
        header_found=''
        for pre in "${prefix[@]}"; do
            # TODO: check that the license is valid as SPDX and is allowed in this project.
            if [[ "$(grep -E -n "${pre}SPDX-License-Identifier: " "${p}")" == "${line}:${pre}SPDX-License-Identifier: "* ]]; then
                header_found='1'
                break
            fi
        done
        if [[ -z "${header_found}" ]]; then
            failed_files+="${p}:${line}"$'\n'
        fi
    done
    if [[ -n "${failed_files}" ]]; then
        error "license-check failed: please add SPDX-License-Identifier to the following files"
        echo "======================================="
        echo -n "${failed_files}"
        echo "======================================="
    fi
fi

# Spell check (if config exists)
if [[ -f .cspell.json ]]; then
    info "spell checking"
    project_dictionary=.github/.cspell/project-dictionary.txt
    if type -P npm &>/dev/null; then
        has_rust=''
        if [[ -n "$(git ls-files '*Cargo.toml')" ]]; then
            has_rust='1'
            dependencies=''
            for manifest_path in $(git ls-files '*Cargo.toml'); do
                if [[ "${manifest_path}" != "Cargo.toml" ]] && ! grep -Eq '\[workspace\]' "${manifest_path}"; then
                    continue
                fi
                metadata=$(cargo metadata --format-version=1 --no-deps --manifest-path "${manifest_path}")
                for id in $(jq <<<"${metadata}" '.workspace_members[]'); do
                    dependencies+="$(jq <<<"${metadata}" ".packages[] | select(.id == ${id})" | jq -r '.dependencies[].name')"$'\n'
                done
            done
            # shellcheck disable=SC2001
            dependencies=$(sed <<<"${dependencies}" 's/[0-9_-]/\n/g' | LC_ALL=C sort -f -u)
        fi
        config_old=$(<.cspell.json)
        config_new=$(grep <<<"${config_old}" -v ' *//' | jq 'del(.dictionaries[] | select(index("organization-dictionary") | not))' | jq 'del(.dictionaryDefinitions[] | select(.name == "organization-dictionary" | not))')
        trap -- 'echo "${config_old}" >.cspell.json; echo >&2 "$0: trapped SIGINT"; exit 1' SIGINT
        echo "${config_new}" >.cspell.json
        if [[ -n "${has_rust}" ]]; then
            dependencies_words=$(npx <<<"${dependencies}" -y cspell stdin --no-progress --no-summary --words-only --unique || true)
        fi
        all_words=$(npx -y cspell --no-progress --no-summary --words-only --unique $(git ls-files | (grep -v "${project_dictionary//\./\\.}" || true)) || true)
        echo "${config_old}" >.cspell.json
        trap - SIGINT
        cat >.github/.cspell/rust-dependencies.txt <<EOF
// This file is @generated by $(basename "$0").
// It is not intended for manual editing.
EOF
        if [[ -n "${dependencies_words:-}" ]]; then
            echo $'\n'"${dependencies_words}" >>.github/.cspell/rust-dependencies.txt
        fi
        check_diff .github/.cspell/rust-dependencies.txt
        if ! grep -Eq "^\.github/\.cspell/rust-dependencies.txt linguist-generated" .gitattributes; then
            echo "warning: you may want to mark .github/.cspell/rust-dependencies.txt linguist-generated"
        fi

        echo "+ npx -y cspell --no-progress --no-summary \$(git ls-files)"
        if ! npx -y cspell --no-progress --no-summary $(git ls-files); then
            error "spellcheck failed: please fix uses of above words or add to ${project_dictionary} if correct"
        fi

        # Make sure the project-specific dictionary does not contain duplicated words.
        for dictionary in .github/.cspell/*.txt; do
            if [[ "${dictionary}" == "${project_dictionary}" ]]; then
                continue
            fi
            dup=$(sed '/^$/d' "${project_dictionary}" "${dictionary}" | LC_ALL=C sort -f | uniq -d -i | (grep -v '//.*' || true))
            if [[ -n "${dup}" ]]; then
                error "duplicated words in dictionaries; please remove the following words from ${project_dictionary}"
                echo "======================================="
                echo "${dup}"
                echo "======================================="
            fi
        done

        # Make sure the project-specific dictionary does not contain unused words.
        unused=''
        for word in $(grep -v '//.*' "${project_dictionary}" || true); do
            if ! grep <<<"${all_words}" -Eq -i "^${word}$"; then
                unused+="${word}"$'\n'
            fi
        done
        if [[ -n "${unused}" ]]; then
            error "unused words in dictionaries; please remove the following words from ${project_dictionary}"
            echo "======================================="
            echo -n "${unused}"
            echo "======================================="
        fi
    else
        warn "'npm' is not installed; skipped spell check"
    fi
fi

if [[ -n "${should_fail:-}" ]]; then
    exit 1
fi
