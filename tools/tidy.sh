#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# shellcheck disable=SC2046
set -CeEuo pipefail
IFS=$'\n\t'
trap -- 's=$?; printf >&2 "%s\n" "${0##*/}:${LINENO}: \`${BASH_COMMAND}\` exit with ${s}"; exit ${s}' ERR
trap -- 'printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
cd -- "$(dirname -- "$0")"/..

# USAGE:
#    ./tools/tidy.sh
#
# Note: This script requires the following tools:
# - git
# - jq 1.6+
# - npm (node 18+)
# - python 3.6+
# - shfmt
# - shellcheck
# - cargo, rustfmt (if Rust code exists)
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
check_config() {
    if [[ ! -e "$1" ]]; then
        error "could not found $1 in the repository root"
    fi
}
check_install() {
    for tool in "$@"; do
        if ! type -P "${tool}" >/dev/null; then
            if [[ "${tool}" == "python3" ]]; then
                if type -P python >/dev/null; then
                    continue
                fi
            fi
            error "'${tool}' is required to run this check"
            return 1
        fi
    done
}
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
error() {
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
        printf '::error::%s\n' "$*"
    else
        printf >&2 'error: %s\n' "$*"
    fi
    should_fail=1
}
warn() {
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
        printf '::warning::%s\n' "$*"
    else
        printf >&2 'warning: %s\n' "$*"
    fi
}
info() {
    printf >&2 'info: %s\n' "$*"
}
sed_rhs_escape() {
    sed 's/\\/\\\\/g; s/\&/\\\&/g; s/\//\\\//g' <<<"$1"
}
venv_install_yq() {
    if [[ ! -e "${venv_bin}/yq${exe}" ]]; then
        if [[ ! -d .venv ]]; then
            "python${py_suffix}" -m venv .venv >&2
        fi
        info "installing yq to .venv using pip${py_suffix}"
        "${venv_bin}/pip${py_suffix}${exe}" install yq >&2
    fi
}

if [[ $# -gt 0 ]]; then
    cat <<EOF
USAGE:
    $0
EOF
    exit 1
fi

exe=''
py_suffix=''
if type -P python3 >/dev/null; then
    py_suffix='3'
fi
venv_bin=.venv/bin
yq() {
    venv_install_yq
    "${venv_bin}/yq${exe}" "$@"
}
tomlq() {
    venv_install_yq
    "${venv_bin}/tomlq${exe}" "$@"
}
case "$(uname -s)" in
    Linux)
        if [[ "$(uname -o)" == "Android" ]]; then
            ostype=android
        else
            ostype=linux
        fi
        ;;
    Darwin) ostype=macos ;;
    FreeBSD) ostype=freebsd ;;
    NetBSD) ostype=netbsd ;;
    OpenBSD) ostype=openbsd ;;
    DragonFly) ostype=dragonfly ;;
    SunOS)
        if [[ "$(/usr/bin/uname -o)" == "illumos" ]]; then
            ostype=illumos
        else
            ostype=solaris
            # Solaris /usr/bin/* are not POSIX-compliant (e.g., grep has no -q, -E, -F),
            # and POSIX-compliant commands are in /usr/xpg{4,6,7}/bin.
            # https://docs.oracle.com/cd/E88353_01/html/E37853/xpg-7.html
            if [[ "${PATH}" != *"/usr/xpg4/bin"* ]]; then
                export PATH="/usr/xpg4/bin:${PATH}"
            fi
            # GNU/BSD grep/sed is required to run some checks, but most checks are okay with other POSIX grep/sed.
            # Solaris /usr/xpg4/bin/grep has -q, -E, -F, but no -o (non-POSIX).
            # Solaris /usr/xpg4/bin/sed has no -E (POSIX.1-2024) yet.
            if type -P ggrep >/dev/null; then
                grep() { ggrep "$@"; }
            fi
            if type -P gsed >/dev/null; then
                sed() { gsed "$@"; }
            fi
        fi
        ;;
    MINGW* | MSYS* | CYGWIN* | Windows_NT)
        ostype=windows
        exe=.exe
        venv_bin=.venv/Scripts
        if type -P jq >/dev/null; then
            # https://github.com/jqlang/jq/issues/1854
            _tmp=$(jq -r .a <<<'{}')
            if [[ "${_tmp}" != "null" ]]; then
                _tmp=$(jq -b -r .a 2>/dev/null <<<'{}' || true)
                if [[ "${_tmp}" == "null" ]]; then
                    jq() { command jq -b "$@"; }
                else
                    jq() { command jq "$@" | tr -d '\r'; }
                fi
                yq() {
                    venv_install_yq
                    "${venv_bin}/yq${exe}" "$@" | tr -d '\r'
                }
                tomlq() {
                    venv_install_yq
                    "${venv_bin}/tomlq${exe}" "$@" | tr -d '\r'
                }
            fi
        fi
        ;;
    *) error "unrecognized os type '$(uname -s)' for \`\$(uname -s)\`" ;;
esac

check_install git
exclude_from_ls_files=()
while IFS=$'\n' read -r line; do exclude_from_ls_files+=("${line}"); done < <({
    find . \! \( -name .git -prune \) \! \( -name target -prune \) \! \( -name .venv -prune \) \! \( -name tmp -prune \) -type l | cut -c3-
    git submodule status | sed 's/^.//' | cut -d' ' -f2
    git ls-files --deleted
} | LC_ALL=C sort -u)
ls_files() {
    comm -23 <(git ls-files "$@" | LC_ALL=C sort) <(printf '%s\n' ${exclude_from_ls_files[@]+"${exclude_from_ls_files[@]}"})
}

# Rust (if exists)
if [[ -n "$(ls_files '*.rs')" ]]; then
    info "checking Rust code style"
    check_config .rustfmt.toml
    if check_install cargo jq python3; then
        # `cargo fmt` cannot recognize files not included in the current workspace and modules
        # defined inside macros, so run rustfmt directly.
        # We need to use nightly rustfmt because we use the unstable formatting options of rustfmt.
        rustc_version=$(rustc -vV | grep -E '^release:' | cut -d' ' -f2)
        if [[ "${rustc_version}" =~ nightly|dev ]] || ! type -P rustup >/dev/null; then
            if type -P rustup >/dev/null; then
                retry rustup component add rustfmt &>/dev/null
            fi
            info "running \`rustfmt \$(git ls-files '*.rs')\`"
            rustfmt $(ls_files '*.rs')
        else
            if type -P rustup >/dev/null; then
                retry rustup component add rustfmt --toolchain nightly &>/dev/null
            fi
            info "running \`rustfmt +nightly \$(git ls-files '*.rs')\`"
            rustfmt +nightly $(ls_files '*.rs')
        fi
        check_diff $(ls_files '*.rs')
        cast_without_turbofish=$(grep -Fn '.cast()' $(ls_files '*.rs') || true)
        if [[ -n "${cast_without_turbofish}" ]]; then
            error "please replace \`.cast()\` with \`.cast::<type_name>()\`:"
            printf '%s\n' "${cast_without_turbofish}"
        fi
        # Sync readme and crate-level doc.
        first=1
        for readme in $(ls_files '*README.md'); do
            if ! grep -Eq '^<!-- tidy:crate-doc:start -->' "${readme}"; then
                continue
            fi
            lib="$(dirname -- "${readme}")/src/lib.rs"
            if [[ -n "${first}" ]]; then
                first=''
                info "checking readme and crate-level doc are synchronized"
            fi
            if ! grep -Eq '^<!-- tidy:crate-doc:end -->' "${readme}"; then
                bail "missing '<!-- tidy:crate-doc:end -->' comment in ${readme}"
            fi
            if ! grep -Eq '^<!-- tidy:crate-doc:start -->' "${lib}"; then
                bail "missing '<!-- tidy:crate-doc:start -->' comment in ${lib}"
            fi
            if ! grep -Eq '^<!-- tidy:crate-doc:end -->' "${lib}"; then
                bail "missing '<!-- tidy:crate-doc:end -->' comment in ${lib}"
            fi
            new=$(tr '\n' '\a' <"${readme}" | grep -Eo '<!-- tidy:crate-doc:start -->.*<!-- tidy:crate-doc:end -->')
            new=$(tr '\n' '\a' <"${lib}" | sed "s/<!-- tidy:crate-doc:start -->.*<!-- tidy:crate-doc:end -->/$(sed_rhs_escape "${new}")/" | tr '\a' '\n')
            printf '%s\n' "${new}" >|"${lib}"
            check_diff "${lib}"
        done
        # Make sure that public Rust crates don't contain executables and binaries.
        executables=''
        binaries=''
        metadata=$(cargo metadata --format-version=1 --no-deps)
        root_manifest=''
        if [[ -f Cargo.toml ]]; then
            root_manifest=$(cargo locate-project --message-format=plain --manifest-path Cargo.toml)
        fi
        exclude=''
        has_public_crate=''
        for pkg in $(jq -c '. as $metadata | .workspace_members[] as $id | $metadata.packages[] | select(.id == $id)' <<<"${metadata}"); do
            eval "$(jq -r '@sh "publish=\(.publish) manifest_path=\(.manifest_path)"' <<<"${pkg}")"
            if [[ "$(tomlq -c '.lints' "${manifest_path}")" == "null" ]]; then
                error "no [lints] table in ${manifest_path} please add '[lints]' with 'workspace = true'"
            fi
            # Publishing is unrestricted if null, and forbidden if an empty array.
            if [[ -z "${publish}" ]]; then
                continue
            fi
            has_public_crate=1
            if [[ "${manifest_path}" == "${root_manifest}" ]]; then
                exclude=$(tomlq -r '.package.exclude[]' "${manifest_path}")
                if ! grep -Eq '^/\.\*$' <<<"${exclude}"; then
                    error "top-level Cargo.toml of non-virtual workspace should have 'exclude' field with \"/.*\""
                fi
                if [[ -e tools ]] && ! grep -Eq '^/tools$' <<<"${exclude}"; then
                    error "top-level Cargo.toml of non-virtual workspace should have 'exclude' field with \"/tools\" if it exists"
                fi
                if [[ -e target-specs ]] && ! grep -Eq '^/target-specs$' <<<"${exclude}"; then
                    error "top-level Cargo.toml of non-virtual workspace should have 'exclude' field with \"/target-specs\" if it exists"
                fi
            fi
        done
        if [[ -n "${has_public_crate}" ]]; then
            info "checking public crates don't contain executables and binaries"
            for p in $(ls_files); do
                # Skip directories.
                if [[ -d "${p}" ]]; then
                    continue
                fi
                # Top-level hidden files/directories and tools/* are excluded from crates.io (ensured by the above check).
                # TODO: fully respect exclude field in Cargo.toml.
                case "${p}" in
                    .* | tools/* | target-specs/*) continue ;;
                esac
                if [[ -x "${p}" ]]; then
                    executables+="${p}"$'\n'
                fi
                # Use diff instead of file because file treats an empty file as a binary
                # https://unix.stackexchange.com/questions/275516/is-there-a-convenient-way-to-classify-files-as-binary-or-text#answer-402870
                if { diff .gitattributes "${p}" || true; } | grep -Eq '^Binary file'; then
                    binaries+="${p}"$'\n'
                fi
            done
            if [[ -n "${executables}" ]]; then
                error "file-permissions-check failed: executables are only allowed to be present in directories that are excluded from crates.io"
                printf '=======================================\n'
                printf '%s' "${executables}"
                printf '=======================================\n'
            fi
            if [[ -n "${binaries}" ]]; then
                error "file-permissions-check failed: binaries are only allowed to be present in directories that are excluded from crates.io"
                printf '=======================================\n'
                printf '%s' "${binaries}"
                printf '=======================================\n'
            fi
        fi
    fi
elif [[ -e .rustfmt.toml ]]; then
    error ".rustfmt.toml is unused"
fi

# C/C++ (if exists)
clang_format_ext=('*.c' '*.h' '*.cpp' '*.hpp')
if [[ -n "$(ls_files "${clang_format_ext[@]}")" ]]; then
    info "checking C/C++ code style"
    check_config .clang-format
    if check_install clang-format; then
        IFS=' '
        info "running \`clang-format -i \$(git ls-files ${clang_format_ext[*]})\`"
        IFS=$'\n\t'
        clang-format -i $(ls_files "${clang_format_ext[@]}")
        check_diff $(ls_files "${clang_format_ext[@]}")
    fi
elif [[ -e .clang-format ]]; then
    error ".clang-format is unused"
fi
# https://gcc.gnu.org/onlinedocs/gcc/Overall-Options.html
cpp_alt_ext=('*.cc' '*.cp' '*.cxx' '*.C' '*.CPP' '*.c++')
hpp_alt_ext=('*.hh' '*.hp' '*.hxx' '*.H' '*.HPP' '*.h++')
if [[ -n "$(ls_files "${cpp_alt_ext[@]}")" ]]; then
    error "please use '.cpp' for consistency"
    printf '=======================================\n'
    ls_files "${cpp_alt_ext[@]}"
    printf '=======================================\n'
fi
if [[ -n "$(ls_files "${hpp_alt_ext[@]}")" ]]; then
    error "please use '.hpp' for consistency"
    printf '=======================================\n'
    ls_files "${hpp_alt_ext[@]}"
    printf '=======================================\n'
fi

# YAML/JavaScript/JSON (if exists)
prettier_ext=('*.yml' '*.yaml' '*.js' '*.json')
if [[ -n "$(ls_files "${prettier_ext[@]}")" ]]; then
    info "checking YAML/JavaScript/JSON code style"
    check_config .editorconfig
    if [[ "${ostype}" == "solaris" ]] && [[ -n "${CI:-}" ]] && ! type -P npm >/dev/null; then
        warn "this check is skipped on Solaris due to no node 18+ in upstream package manager"
    elif check_install npm; then
        IFS=' '
        info "running \`npx -y prettier -l -w \$(git ls-files ${prettier_ext[*]})\`"
        IFS=$'\n\t'
        npx -y prettier -l -w $(ls_files "${prettier_ext[@]}")
        check_diff $(ls_files "${prettier_ext[@]}")
    fi
fi
if [[ -n "$(ls_files '*.yaml' | { grep -Fv '.markdownlint-cli2.yaml' || true; })" ]]; then
    error "please use '.yml' instead of '.yaml' for consistency"
    printf '=======================================\n'
    ls_files '*.yaml' | { grep -Fv '.markdownlint-cli2.yaml' || true; }
    printf '=======================================\n'
fi

# TOML (if exists)
if [[ -n "$(ls_files '*.toml' | { grep -Fv '.taplo.toml' || true; })" ]]; then
    info "checking TOML style"
    check_config .taplo.toml
    if [[ "${ostype}" == "solaris" ]] && [[ -n "${CI:-}" ]] && ! type -P npm >/dev/null; then
        warn "this check is skipped on Solaris due to no node 18+ in upstream package manager"
    elif check_install npm; then
        info "running \`npx -y @taplo/cli fmt \$(git ls-files '*.toml')\`"
        RUST_LOG=warn npx -y @taplo/cli fmt $(ls_files '*.toml')
        check_diff $(ls_files '*.toml')
    fi
elif [[ -e .taplo.toml ]]; then
    error ".taplo.toml is unused"
fi

# Markdown (if exists)
if [[ -n "$(ls_files '*.md')" ]]; then
    info "checking Markdown style"
    check_config .markdownlint-cli2.yaml
    if [[ "${ostype}" == "solaris" ]] && [[ -n "${CI:-}" ]] && ! type -P npm >/dev/null; then
        warn "this check is skipped on Solaris due to no node 18+ in upstream package manager"
    elif check_install npm; then
        info "running \`npx -y markdownlint-cli2 \$(git ls-files '*.md')\`"
        if ! npx -y markdownlint-cli2 $(ls_files '*.md'); then
            should_fail=1
        fi
    fi
elif [[ -e .markdownlint-cli2.yaml ]]; then
    error ".markdownlint-cli2.yaml is unused"
fi
if [[ -n "$(ls_files '*.markdown')" ]]; then
    error "please use '.md' instead of '.markdown' for consistency"
    printf '=======================================\n'
    ls_files '*.markdown'
    printf '=======================================\n'
fi

# Shell scripts
info "checking Shell scripts"
shell_files=()
docker_files=()
bash_files=()
grep_ere_files=()
sed_ere_files=()
for p in $(ls_files '*.sh' '*Dockerfile*'); do
    case "${p##*/}" in
        *.sh)
            shell_files+=("${p}")
            if [[ "$(head -1 "${p}")" =~ ^#!/.*bash ]]; then
                bash_files+=("${p}")
            fi
            ;;
        *Dockerfile*)
            docker_files+=("${p}")
            bash_files+=("${p}") # TODO
            ;;
    esac
    if grep -Eq '(^|[^0-9A-Za-z\."'\''-])(grep) -[A-Za-z]*E[^\)]' "${p}"; then
        grep_ere_files+=("${p}")
    fi
    if grep -Eq '(^|[^0-9A-Za-z\."'\''-])(sed) -[A-Za-z]*E[^\)]' "${p}"; then
        sed_ere_files+=("${p}")
    fi
done
# TODO: .cirrus.yml
workflows=()
actions=()
if [[ -d .github/workflows ]]; then
    for p in .github/workflows/*.yml; do
        workflows+=("${p}")
        bash_files+=("${p}") # TODO
    done
fi
if [[ -n "$(ls_files '*action.yml')" ]]; then
    for p in $(ls_files '*action.yml'); do
        if [[ "${p##*/}" == "action.yml" ]]; then
            actions+=("${p}")
            if ! grep -Fq 'shell: sh' "${p}"; then
                bash_files+=("${p}")
            fi
        fi
    done
fi
# correctness
res=$({ grep -En '(\[\[ .* ]]|(^|[^\$])\(\(.*\)\))( +#| *$)' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
    error "bare [[ ]] and (( )) may not work as intended: see https://github.com/koalaman/shellcheck/issues/2360 for more"
    printf '=======================================\n'
    printf '%s\n' "${res}"
    printf '=======================================\n'
fi
# TODO: chmod|chown
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(basename|cat|cd|cp|dirname|ln|ls|mkdir|mv|pushd|rm|rmdir|tee|touch)( +-[0-9A-Za-z]+)* +[^<>\|-]' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
    error "use \`--\` before path(s): see https://github.com/koalaman/shellcheck/issues/2707 / https://github.com/koalaman/shellcheck/issues/2612 / https://github.com/koalaman/shellcheck/issues/2305 / https://github.com/koalaman/shellcheck/issues/2157 / https://github.com/koalaman/shellcheck/issues/2121 / https://github.com/koalaman/shellcheck/issues/314 for more"
    printf '=======================================\n'
    printf '%s\n' "${res}"
    printf '=======================================\n'
fi
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(LINES|RANDOM|PWD)=' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
    error "do not modify these built-in bash variables: see https://github.com/koalaman/shellcheck/issues/2160 / https://github.com/koalaman/shellcheck/issues/2559 for more"
    printf '=======================================\n'
    printf '%s\n' "${res}"
    printf '=======================================\n'
fi
# perf
res=$({ grep -En '(^|[^\\])\$\((cat) ' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
    error "use faster \`\$(<file)\` instead of \$(cat -- file): see https://github.com/koalaman/shellcheck/issues/2493 for more"
    printf '=======================================\n'
    printf '%s\n' "${res}"
    printf '=======================================\n'
fi
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(command +-[vV]) ' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
    error "use faster \`type -P\` instead of \`command -v\`: see https://github.com/koalaman/shellcheck/issues/1162 for more"
    printf '=======================================\n'
    printf '%s\n' "${res}"
    printf '=======================================\n'
fi
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(type) +-P +[^ ]+ +&>' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
    error "\`type -P\` doesn't output to stderr; use \`>\` instead of \`&>\`"
    printf '=======================================\n'
    printf '%s\n' "${res}"
    printf '=======================================\n'
fi
# TODO: multi-line case
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(echo|printf )[^;)]* \|[^\|]' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
    error "use faster \`<<<...\` instead of \`echo ... |\`/\`printf ... |\`: see https://github.com/koalaman/shellcheck/issues/2593 for more"
    printf '=======================================\n'
    printf '%s\n' "${res}"
    printf '=======================================\n'
fi
# style
if [[ ${#grep_ere_files[@]} -gt 0 ]]; then
    # We intentionally do not check for occurrences in any other order (e.g., -iE, -i -E) here.
    # This enforces the style and makes it easier to search.
    res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(grep) +([^-]|-[^EFP-]|--[^hv])' "${grep_ere_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
    if [[ -n "${res}" ]]; then
        error "please always use ERE (grep -E) instead of BRE for code consistency within a file"
        printf '=======================================\n'
        printf '%s\n' "${res}"
        printf '=======================================\n'
    fi
fi
if [[ ${#sed_ere_files[@]} -gt 0 ]]; then
    res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(sed) +([^-]|-[^E-]|--[^hv])' "${sed_ere_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
    if [[ -n "${res}" ]]; then
        error "please always use ERE (sed -E) instead of BRE for code consistency within a file"
        printf '=======================================\n'
        printf '%s\n' "${res}"
        printf '=======================================\n'
    fi
fi
if check_install shfmt; then
    check_config .editorconfig
    info "running \`shfmt -l -w \$(git ls-files '*.sh')\`"
    if ! shfmt -l -w "${shell_files[@]}"; then
        should_fail=1
    fi
    check_diff "${shell_files[@]}"
fi
if [[ "${ostype}" == "solaris" ]] && [[ -n "${CI:-}" ]] && ! type -P shellcheck >/dev/null; then
    warn "this check is skipped on Solaris due to no haskell/shellcheck in upstream package manager"
elif check_install shellcheck; then
    check_config .shellcheckrc
    info "running \`shellcheck \$(git ls-files '*.sh')\`"
    if ! shellcheck "${shell_files[@]}"; then
        should_fail=1
    fi
    if [[ ${#docker_files[@]} -gt 0 ]]; then
        # SC2154 doesn't seem to work on dockerfile.
        # SC2250 may not correct on dockerfile because $v and ${v} is sometime different: https://github.com/moby/moby/issues/42863
        info "running \`shellcheck --shell bash --exclude SC2154,SC2250 \$(git ls-files '*Dockerfile*')\`"
        if ! shellcheck --shell bash --exclude SC2154,SC2250 "${docker_files[@]}"; then
            should_fail=1
        fi
    fi
    # Check scripts in other files.
    if [[ ${#workflows[@]} -gt 0 ]] || [[ ${#actions[@]} -gt 0 ]]; then
        info "running \`shellcheck --exclude SC2086,SC2096,SC2129\` for scripts in .github/workflows/*.yml and **/action.yml"
        if [[ "${ostype}" == "windows" ]]; then
            # No such file or directory: '/proc/N/fd/N'
            warn "this check is skipped on Windows due to upstream bug (failed to found fd created by <())"
        elif [[ "${ostype}" == "dragonfly" ]]; then
            warn "this check is skipped on DragonFly BSD due to upstream bug (hang)"
        elif check_install jq python3; then
            shellcheck_for_gha() {
                local text=$1
                local shell=$2
                local display_path=$3
                if [[ "${text}" == "null" ]]; then
                    return
                fi
                case "${shell}" in
                    bash* | sh*) ;;
                    *) return ;;
                esac
                # Use python because sed doesn't support .*?.
                text=$(
                    "python${py_suffix}" - <(printf '%s\n%s' "#!/usr/bin/env ${shell%' {0}'}" "${text}") <<EOF
import re
import sys
with open(sys.argv[1], 'r') as f:
    text = f.read()
text = re.sub(r"\\\${{.*?}}", "\${__GHA_SYNTAX__}", text)
print(text)
EOF
                )
                case "${ostype}" in
                    windows) text=${text//\r/} ;;
                esac
                local color=auto
                if [[ -t 1 ]] || [[ -n "${GITHUB_ACTIONS:-}" ]]; then
                    color=always
                fi
                if ! shellcheck --color="${color}" --exclude SC2086,SC2096,SC2129 <(printf '%s\n' "${text}") | sed "s/\/dev\/fd\/[0-9][0-9]*/$(sed_rhs_escape "${display_path}")/g"; then
                    should_fail=1
                fi
            }
            for workflow_path in ${workflows[@]+"${workflows[@]}"}; do
                workflow=$(yq -c '.' "${workflow_path}")
                # The top-level permissions must be weak as they are referenced by all jobs.
                permissions=$(jq -c '.permissions' <<<"${workflow}")
                case "${permissions}" in
                    '{"contents":"read"}' | '{"contents":"none"}') ;;
                    null) error "${workflow_path}: top level permissions not found; it must be 'contents: read' or weaker permissions" ;;
                    *) error "${workflow_path}: only 'contents: read' and weaker permissions are allowed at top level, but found '${permissions}'; if you want to use stronger permissions, please set job-level permissions" ;;
                esac
                default_shell=$(jq -r -c '.defaults.run.shell' <<<"${workflow}")
                # github's default is https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#defaultsrunshell
                if [[ ! "${default_shell}" =~ ^bash\ --noprofile\ --norc\ -CeEux?o\ pipefail\ \{0}$ ]]; then
                    error "${workflow_path}: defaults.run.shell should be 'bash --noprofile --norc -CeEuxo pipefail {0}' or 'bash --noprofile --norc -CeEuo pipefail {0}'"
                    continue
                fi
                # .steps == null means the job is the caller of reusable workflow
                for job in $(jq -c '.jobs | to_entries[] | select(.value.steps)' <<<"${workflow}"); do
                    name=$(jq -r '.key' <<<"${job}")
                    job=$(jq -r '.value' <<<"${job}")
                    n=0
                    job_default_shell=$(jq -r '.defaults.run.shell' <<<"${job}")
                    if [[ "${job_default_shell}" == "null" ]]; then
                        job_default_shell="${default_shell}"
                    fi
                    for step in $(jq -c '.steps[]' <<<"${job}"); do
                        prepare=''
                        eval "$(jq -r 'if .run then @sh "RUN=\(.run) shell=\(.shell)" else @sh "RUN=\(.with.run) prepare=\(.with.prepare) shell=\(.with.shell)" end' <<<"${step}")"
                        if [[ "${RUN}" == "null" ]]; then
                            _=$((n++))
                            continue
                        fi
                        if [[ "${shell}" == "null" ]]; then
                            if [[ -z "${prepare}" ]]; then
                                shell="${job_default_shell}"
                            elif grep -Eq '^ *chsh +-s +[^ ]+/bash' <<<"${prepare}"; then
                                shell='bash'
                            else
                                shell='sh'
                            fi
                        fi
                        shellcheck_for_gha "${RUN}" "${shell}" "${workflow_path} ${name}.steps[${n}].run"
                        shellcheck_for_gha "${prepare:-null}" 'sh' "${workflow_path} ${name}.steps[${n}].run"
                        _=$((n++))
                    done
                done
            done
            for action_path in ${actions[@]+"${actions[@]}"}; do
                runs=$(yq -c '.runs' "${action_path}")
                if [[ "$(jq -r '.using' <<<"${runs}")" != "composite" ]]; then
                    continue
                fi
                n=0
                for step in $(jq -c '.steps[]' <<<"${runs}"); do
                    prepare=''
                    eval "$(jq -r 'if .run then @sh "RUN=\(.run) shell=\(.shell)" else @sh "RUN=\(.with.run) prepare=\(.with.prepare) shell=\(.with.shell)" end' <<<"${step}")"
                    if [[ "${RUN}" == "null" ]]; then
                        _=$((n++))
                        continue
                    fi
                    if [[ "${shell}" == "null" ]]; then
                        if [[ -z "${prepare}" ]]; then
                            error "\`shell: ..\` is required"
                            continue
                        elif grep -Eq '^ *chsh +-s +[^ ]+/bash' <<<"${prepare}"; then
                            shell='bash'
                        else
                            shell='sh'
                        fi
                    fi
                    shellcheck_for_gha "${RUN}" "${shell}" "${action_path} steps[${n}].run"
                    shellcheck_for_gha "${prepare:-null}" 'sh' "${action_path} steps[${n}].run"
                    _=$((n++))
                done
            done
        fi
    fi
fi

# License check
# TODO: This check is still experimental and does not track all files that should be tracked.
if [[ -f tools/.tidy-check-license-headers ]]; then
    info "checking license headers (experimental)"
    failed_files=''
    for p in $(comm -12 <(eval $(<tools/.tidy-check-license-headers) | LC_ALL=C sort) <(ls_files | LC_ALL=C sort)); do
        case "${p##*/}" in
            *.stderr | *.expanded.rs) continue ;; # generated files
            *.sh | *.py | *.rb | *Dockerfile*) prefix=("# ") ;;
            *.rs | *.c | *.h | *.cpp | *.hpp | *.s | *.S | *.js) prefix=("// " "/* ") ;;
            *.ld | *.x) prefix=("/* ") ;;
            # TODO: More file types?
            *) continue ;;
        esac
        # TODO: The exact line number is not actually important; it is important
        # that it be part of the top-level comments of the file.
        line=1
        if IFS= LC_ALL=C read -rd '' -n3 shebang <"${p}" && [[ "${shebang}" == '#!/' ]]; then
            line=2
        elif [[ "${p}" == *"Dockerfile"* ]] && IFS= LC_ALL=C read -rd '' -n9 syntax <"${p}" && [[ "${syntax}" == '# syntax=' ]]; then
            line=2
        fi
        header_found=''
        for pre in "${prefix[@]}"; do
            # TODO: check that the license is valid as SPDX and is allowed in this project.
            if [[ "$(grep -Fn "${pre}SPDX-License-Identifier: " "${p}")" == "${line}:${pre}SPDX-License-Identifier: "* ]]; then
                header_found=1
                break
            fi
        done
        if [[ -z "${header_found}" ]]; then
            failed_files+="${p}:${line}"$'\n'
        fi
    done
    if [[ -n "${failed_files}" ]]; then
        error "license-check failed: please add SPDX-License-Identifier to the following files"
        printf '=======================================\n'
        printf '%s' "${failed_files}"
        printf '=======================================\n'
    fi
fi

# Spell check (if config exists)
if [[ -f .cspell.json ]]; then
    info "spell checking"
    project_dictionary=.github/.cspell/project-dictionary.txt
    if [[ "${ostype}" == "solaris" ]] && [[ -n "${CI:-}" ]] && ! type -P npm >/dev/null; then
        warn "this check is skipped on Solaris due to no node 18+ in upstream package manager"
    elif [[ "${ostype}" == "illumos" ]]; then
        warn "this check is skipped on illumos due to upstream bug (dictionaries are not loaded correctly)"
    elif check_install npm jq python3; then
        has_rust=''
        if [[ -n "$(ls_files '*Cargo.toml')" ]]; then
            has_rust=1
            dependencies=''
            for manifest_path in $(ls_files '*Cargo.toml'); do
                if [[ "${manifest_path}" != "Cargo.toml" ]] && [[ "$(tomlq -c '.workspace' "${manifest_path}")" == "null" ]]; then
                    continue
                fi
                dependencies+="$(cargo metadata --format-version=1 --no-deps --manifest-path "${manifest_path}" | jq -r '. as $metadata | .workspace_members[] as $id | $metadata.packages[] | select(.id == $id) | .dependencies[].name')"$'\n'
            done
            dependencies=$(LC_ALL=C sort -f -u <<<"${dependencies//[0-9_-]/$'\n'}")
        fi
        config_old=$(<.cspell.json)
        config_new=$(grep -Ev '^ *//' <<<"${config_old}" | jq 'del(.dictionaries[] | select(index("organization-dictionary") | not)) | del(.dictionaryDefinitions[] | select(.name == "organization-dictionary" | not))')
        trap -- 'printf "%s\n" "${config_old}" >|.cspell.json; printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
        printf '%s\n' "${config_new}" >|.cspell.json
        dependencies_words=''
        if [[ -n "${has_rust}" ]]; then
            dependencies_words=$(npx -y cspell stdin --no-progress --no-summary --words-only --unique <<<"${dependencies}" || true)
        fi
        all_words=$(npx -y cspell --no-progress --no-summary --words-only --unique $(ls_files | { grep -Fv "${project_dictionary}" || true; }) || true)
        printf '%s\n' "${config_old}" >|.cspell.json
        trap -- 'printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
        cat >|.github/.cspell/rust-dependencies.txt <<EOF
// This file is @generated by ${0##*/}.
// It is not intended for manual editing.
EOF
        if [[ -n "${dependencies_words}" ]]; then
            LC_ALL=C sort -f >>.github/.cspell/rust-dependencies.txt <<<"${dependencies_words}"$'\n'
        fi
        check_diff .github/.cspell/rust-dependencies.txt
        if ! grep -Fq '.github/.cspell/rust-dependencies.txt linguist-generated' .gitattributes; then
            error "you may want to mark .github/.cspell/rust-dependencies.txt linguist-generated"
        fi

        info "running \`npx -y cspell --no-progress --no-summary \$(git ls-files)\`"
        if ! npx -y cspell --no-progress --no-summary $(ls_files); then
            error "spellcheck failed: please fix uses of below words or add to ${project_dictionary} if correct"
            printf '=======================================\n'
            { npx -y cspell --no-progress --no-summary --words-only $(git ls-files) || true; } | LC_ALL=C sort -f -u
            printf '=======================================\n\n'
        fi

        # Make sure the project-specific dictionary does not contain duplicated words.
        for dictionary in .github/.cspell/*.txt; do
            if [[ "${dictionary}" == "${project_dictionary}" ]]; then
                continue
            fi
            case "${ostype}" in
                # NetBSD uniq doesn't support -i flag.
                netbsd) dup=$(sed '/^$/d; /^\/\//d' "${project_dictionary}" "${dictionary}" | LC_ALL=C sort -f | tr '[:upper:]' '[:lower:]' | LC_ALL=C uniq -d) ;;
                *) dup=$(sed '/^$/d; /^\/\//d' "${project_dictionary}" "${dictionary}" | LC_ALL=C sort -f | LC_ALL=C uniq -d -i) ;;
            esac
            if [[ -n "${dup}" ]]; then
                error "duplicated words in dictionaries; please remove the following words from ${project_dictionary}"
                printf '=======================================\n'
                printf '%s\n' "${dup}"
                printf '=======================================\n\n'
            fi
        done

        # Make sure the project-specific dictionary does not contain unused words.
        if [[ -n "${REMOVE_UNUSED_WORDS:-}" ]]; then
            grep_args=()
            for word in $(grep -Ev '^//.*' "${project_dictionary}" || true); do
                if ! grep -Eqi "^${word}$" <<<"${all_words}"; then
                    grep_args+=(-e "^${word}$")
                fi
            done
            if [[ ${#grep_args[@]} -gt 0 ]]; then
                info "removing unused words from ${project_dictionary}"
                res=$(grep -Ev "${grep_args[@]}" "${project_dictionary}")
                printf '%s\n' "${res}" >|"${project_dictionary}"
            fi
        else
            unused=''
            for word in $(grep -Ev '^//.*' "${project_dictionary}" || true); do
                if ! grep -Eqi "^${word}$" <<<"${all_words}"; then
                    unused+="${word}"$'\n'
                fi
            done
            if [[ -n "${unused}" ]]; then
                error "unused words in dictionaries; please remove the following words from ${project_dictionary} or run ${0##*/} with REMOVE_UNUSED_WORDS=1"
                printf '=======================================\n'
                printf '%s' "${unused}"
                printf '=======================================\n'
            fi
        fi
    fi
fi

if [[ -n "${should_fail:-}" ]]; then
    exit 1
fi
