#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# shellcheck disable=SC2046
set -CeEuo pipefail
IFS=$'\n\t'
trap -- 's=$?; printf >&2 "%s\n" "${0##*/}:${LINENO}: \`${BASH_COMMAND}\` exit with ${s}"; exit ${s}' ERR
trap -- 'printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
cd -- "$(dirname -- "$0")"/..

# USAGE:
#    GH_TOKEN=$(gh auth token) ./tools/tidy.sh
#
# Note: This script requires the following tools:
# - git 1.8+
# - jq 1.6+
# - npm (node 18+)
# - python 3.6+ and pipx
# - shfmt
# - shellcheck
# - zizmor
# - cargo, rustfmt (if Rust code exists)
# - clang-format (if C/C++/Protobuf code exists)
# - parse-dockerfile <https://github.com/taiki-e/parse-dockerfile> (if Dockerfile exists)
#
# This script is shared by projects under github.com/taiki-e, so there may also
# be checks for files not included in this repository, but they will be skipped
# if the corresponding files do not exist.
# It is not intended for manual editing.

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
print_fenced() {
  printf '=======================================\n'
  printf '%s' "$*"
  printf '=======================================\n\n'
}
check_diff() {
  if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
    if ! git -c color.ui=always --no-pager diff --exit-code "$@"; then
      should_fail=1
    fi
  elif [[ -n "${CI:-}" ]]; then
    if ! git --no-pager diff --exit-code "$@"; then
      should_fail=1
    fi
  else
    local res
    res=$(git --no-pager diff --exit-code --name-only "$@" || true)
    if [[ -n "${res}" ]]; then
      warn "please commit changes made by formatter/generator if exists on the following files"
      print_fenced "${res}"$'\n'
      should_fail=1
    fi
  fi
}
check_config() {
  if [[ ! -e "$1" ]]; then
    error "could not found $1 in the repository root${2:-}"
  fi
}
check_install() {
  for tool in "$@"; do
    if ! type -P "${tool}" >/dev/null; then
      if [[ "${tool}" == 'python3' ]]; then
        if type -P python >/dev/null; then
          continue
        fi
      fi
      error "'${tool}' is required to run this check"
      return 1
    fi
  done
}
check_unused() {
  local kind="$1"
  shift
  local res
  res=$(ls_files "$@")
  if [[ -n "${res}" ]]; then
    error "the following files are unused because there is no ${kind}; consider removing them"
    print_fenced "${res}"$'\n'
  fi
}
check_alt() {
  local recommended=$1
  local not_recommended=$2
  if [[ -n "$3" ]]; then
    error "please use ${recommended} instead of ${not_recommended} for consistency"
    print_fenced "$3"$'\n'
  fi
}
check_hidden() {
  local res
  for file in "$@"; do
    check_alt ".${file}" "${file}" "$(LC_ALL=C comm -23 <(ls_files "*${file}") <(ls_files "*.${file}"))"
  done
}
sed_rhs_escape() {
  sed -E 's/\\/\\\\/g; s/\&/\\\&/g; s/\//\\\//g' <<<"$1"
}

if [[ $# -gt 0 ]]; then
  cat <<EOF
USAGE:
    $0
EOF
  exit 1
fi

py_suffix=''
if type -P python3 >/dev/null; then
  py_suffix=3
fi
yq() { pipx run yq "$@"; }
tomlq() { pipx run --spec yq tomlq "$@"; }
case "$(uname -s)" in
  Linux)
    if [[ "$(uname -o)" == 'Android' ]]; then
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
    if [[ "$(/usr/bin/uname -o)" == 'illumos' ]]; then
      ostype=illumos
    else
      ostype=solaris
      # Solaris /usr/bin/* are not POSIX-compliant (e.g., grep has no -q, -E, -F),
      # and POSIX-compliant commands are in /usr/xpg{4,6,7}/bin.
      # https://docs.oracle.com/cd/E88353_01/html/E37853/xpg-7.html
      if [[ "${PATH}" != *'/usr/xpg4/bin'* ]]; then
        export PATH="/usr/xpg4/bin:${PATH}"
      fi
      # GNU/BSD sed is required.
      # GNU/BSD grep is required by some checks, but most checks are okay with other POSIX grep.
      # Solaris /usr/xpg4/bin/grep has -q, -E, -F, but no -o (non-POSIX).
      # Solaris /usr/xpg4/bin/sed has no -E (POSIX.1-2024) yet.
      for tool in 'grep' 'sed'; do
        if type -P "g${tool}" >/dev/null; then
          eval "${tool}() { g${tool} \"\$@\"; }"
        fi
      done
    fi
    ;;
  MINGW* | MSYS* | CYGWIN* | Windows_NT)
    ostype=windows
    if type -P jq >/dev/null; then
      # https://github.com/jqlang/jq/issues/1854
      _tmp=$(jq -r .a <<<'{}')
      if [[ "${_tmp}" != 'null' ]]; then
        _tmp=$(jq -b -r .a 2>/dev/null <<<'{}' || true)
        if [[ "${_tmp}" == 'null' ]]; then
          jq() { command jq -b "$@"; }
        else
          jq() { command jq "$@" | tr -d '\r'; }
        fi
        yq() { pipx run yq "$@" | tr -d '\r'; }
        tomlq() { pipx run --spec yq tomlq "$@" | tr -d '\r'; }
      fi
    fi
    ;;
  *) error "unrecognized os type '$(uname -s)' for \`\$(uname -s)\`" ;;
esac

check_install git
exclude_from_ls_files=()
# - `find` lists symlinks. `! ( -name <dir> -prune )` means recursively ignore <dir>. `cut` removes the leading `./`.
#   This can be replaced with `fd -H -t l`.
# - `git submodule status` lists submodules. The first `cut` removes the first character indicates status ( |+|-).
# - `git ls-files --deleted` lists removed files.
find_prune=(\! \( -name .git -prune \))
while IFS= read -r; do
  find_prune+=(\! \( -name "${REPLY}" -prune \))
done < <(sed -E 's/#.*//g; s/^[ \t]+//g; s/\/[ \t]+$//g; /^$/d' .gitignore)
while IFS=$'\n' read -r; do
  exclude_from_ls_files+=("${REPLY}")
done < <({
  find . "${find_prune[@]}" -type l | cut -c3-
  git submodule status | cut -c2- | cut -d' ' -f2
  git ls-files --deleted
} | LC_ALL=C sort -u)
exclude_from_ls_files_no_symlink=()
while IFS=$'\n' read -r; do
  exclude_from_ls_files_no_symlink+=("${REPLY}")
done < <({
  git submodule status | cut -c2- | cut -d' ' -f2
  git ls-files --deleted
} | LC_ALL=C sort -u)
ls_files() {
  if [[ "${1:-}" == '--include-symlink' ]]; then
    shift
    LC_ALL=C comm -23 <(git ls-files "$@" | LC_ALL=C sort) <(printf '%s\n' ${exclude_from_ls_files_no_symlink[@]+"${exclude_from_ls_files_no_symlink[@]}"})
  else
    LC_ALL=C comm -23 <(git ls-files "$@" | LC_ALL=C sort) <(printf '%s\n' ${exclude_from_ls_files[@]+"${exclude_from_ls_files[@]}"})
  fi
}

# Rust (if exists)
if [[ -n "$(ls_files '*.rs')" ]]; then
  info "checking Rust code style"
  check_config .rustfmt.toml "; consider adding with reference to https://github.com/taiki-e/cargo-hack/blob/HEAD/.rustfmt.toml"
  check_config .clippy.toml "; consider adding with reference to https://github.com/taiki-e/cargo-hack/blob/HEAD/.clippy.toml"
  if check_install cargo jq python3 pipx; then
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
    has_root_crate=''
    for pkg in $(jq -c '. as $metadata | .workspace_members[] as $id | $metadata.packages[] | select(.id == $id)' <<<"${metadata}"); do
      eval "$(jq -r '@sh "publish=\(.publish) manifest_path=\(.manifest_path)"' <<<"${pkg}")"
      if [[ "$(tomlq -c '.lints' "${manifest_path}")" == 'null' ]]; then
        error "no [lints] table in ${manifest_path} please add '[lints]' with 'workspace = true'"
      fi
      # Publishing is unrestricted if null, and forbidden if an empty array.
      if [[ -z "${publish}" ]]; then
        continue
      fi
      has_public_crate=1
      if [[ "${manifest_path}" == "${root_manifest}" ]]; then
        has_root_crate=1
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
      check_config .deny.toml "; consider adding with reference to https://github.com/taiki-e/cargo-hack/blob/HEAD/.deny.toml"
      info "checking public crates don't contain executables and binaries"
      for p in $(ls_files --include-symlink); do
        # Skip directories.
        if [[ -d "${p}" ]]; then
          continue
        fi
        # Top-level hidden files/directories and tools/* are excluded from crates.io (ensured by the above check).
        # TODO: fully respect exclude field in Cargo.toml.
        case "${p}" in
          .* | tools/* | target-specs/*) continue ;;
          */*) ;;
          *)
            # If there is no crate at root, executables at the repository root directory if always okay.
            if [[ -z "${has_root_crate}" ]]; then
              continue
            fi
            ;;
        esac
        if [[ -x "${p}" ]]; then
          executables+="${p}"$'\n'
        fi
        # Use `diff` instead of `file` because `file` treats an empty file as a binary.
        # https://unix.stackexchange.com/questions/275516/is-there-a-convenient-way-to-classify-files-as-binary-or-text#answer-402870
        if { diff .gitattributes "${p}" || true; } | grep -Eq '^Binary file'; then
          binaries+="${p}"$'\n'
        fi
      done
      if [[ -n "${executables}" ]]; then
        error "file-permissions-check failed: executables are only allowed to be present in directories that are excluded from crates.io"
        print_fenced "${executables}"
      fi
      if [[ -n "${binaries}" ]]; then
        error "file-permissions-check failed: binaries are only allowed to be present in directories that are excluded from crates.io"
        print_fenced "${binaries}"
      fi
    fi
  fi
  # Sync markdown to rustdoc.
  first=1
  for markdown in $(ls_files '*.md'); do
    markers=$(grep -En '^<!-- tidy:sync-markdown-to-rustdoc:(start[^ ]*|end) -->' "${markdown}" || true)
    # BSD wc's -l emits spaces before number.
    if [[ ! "$(LC_ALL=C wc -l <<<"${markers}")" =~ ^\ *2$ ]]; then
      if [[ -n "${markers}" ]]; then
        error "inconsistent '<!-- tidy:sync-markdown-to-rustdoc:* -->' marker found in ${markdown}"
        printf '%s\n' "${markers}"
      fi
      continue
    fi
    start_marker=$(head -n1 <<<"${markers}")
    end_marker=$(head -n2 <<<"${markers}" | tail -n1)
    if [[ "${start_marker}" == *"tidy:sync-markdown-to-rustdoc:end"* ]] || [[ "${end_marker}" == *"tidy:sync-markdown-to-rustdoc:start"* ]]; then
      error "inconsistent '<!-- tidy:sync-markdown-to-rustdoc:* -->' marker found in ${markdown}"
      printf '%s\n' "${markers}"
      continue
    fi
    if [[ -n "${first}" ]]; then
      first=''
      info "syncing markdown to rustdoc"
    fi
    lib="${start_marker#*:<\!-- tidy:sync-markdown-to-rustdoc:start:}"
    if [[ "${start_marker}" == "${lib}" ]]; then
      error "missing path in '<!-- tidy:sync-markdown-to-rustdoc:start:<path> -->' marker in ${markdown}"
      printf '%s\n' "${markers}"
      continue
    fi
    lib="${lib% -->}"
    lib="$(dirname -- "${markdown}")/${lib}"
    markers=$(grep -En '^<!-- tidy:sync-markdown-to-rustdoc:(start[^ ]*|end) -->' "${lib}" || true)
    # BSD wc's -l emits spaces before number.
    if [[ ! "$(LC_ALL=C wc -l <<<"${markers}")" =~ ^\ *2$ ]]; then
      if [[ -n "${markers}" ]]; then
        error "inconsistent '<!-- tidy:sync-markdown-to-rustdoc:* -->' marker found in ${lib}"
        printf '%s\n' "${markers}"
      else
        error "missing '<!-- tidy:sync-markdown-to-rustdoc:* -->' marker in ${lib}"
      fi
      continue
    fi
    start_marker=$(head -n1 <<<"${markers}")
    end_marker=$(head -n2 <<<"${markers}" | tail -n1)
    if [[ "${start_marker}" == *"tidy:sync-markdown-to-rustdoc:end"* ]] || [[ "${end_marker}" == *"tidy:sync-markdown-to-rustdoc:start"* ]]; then
      error "inconsistent '<!-- tidy:sync-markdown-to-rustdoc:* -->' marker found in ${lib}"
      printf '%s\n' "${markers}"
      continue
    fi
    new='<!-- tidy:sync-markdown-to-rustdoc:start -->'$'\a'
    empty_line_re='^ *$'
    gfm_alert_re='^> {0,4}\[!.*\] *$'
    rust_code_block_re='^ *```(rust|rs) *$'
    code_block_attr=''
    in_alert=''
    first_line=1
    ignore=''
    while IFS='' read -rd$'\a' line; do
      if [[ -n "${ignore}" ]]; then
        if [[ "${line}" == '<!-- tidy:sync-markdown-to-rustdoc:ignore:end -->'* ]]; then
          ignore=''
        fi
        continue
      fi
      if [[ -n "${first_line}" ]]; then
        # Ignore start marker.
        first_line=''
        continue
      elif [[ -n "${in_alert}" ]]; then
        if [[ "${line}" =~ ${empty_line_re} ]]; then
          in_alert=''
          new+=$'\a'"</div>"$'\a'
        fi
      elif [[ "${line}" =~ ${gfm_alert_re} ]]; then
        alert="${line#*[\!}"
        alert="${alert%%]*}"
        alert=$(tr '[:lower:]' '[:upper:]' <<<"${alert%%]*}")
        alert_lower=$(tr '[:upper:]' '[:lower:]' <<<"${alert}")
        case "${alert}" in
          NOTE | TIP | IMPORTANT) alert_sign='ⓘ' ;;
          WARNING | CAUTION) alert_sign='⚠' ;;
          *)
            error "unknown alert type '${alert}' found; please use one of the types listed in <https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts>"
            new+="${line}"$'\a'
            continue
            ;;
        esac
        in_alert=1
        new+="<div class=\"rustdoc-alert rustdoc-alert-${alert_lower}\">"$'\a\a'
        new+="> **${alert_sign} ${alert:0:1}${alert_lower:1}**"$'\a>\a'
        continue
      fi
      if [[ "${line}" =~ ${rust_code_block_re} ]]; then
        code_block_attr="${code_block_attr#<\!-- tidy:sync-markdown-to-rustdoc:code-block:}"
        code_block_attr="${code_block_attr%% -->*}"
        new+="${line/\`\`\`*/\`\`\`}${code_block_attr}"$'\a'
        code_block_attr=''
        continue
      fi
      if [[ -n "${code_block_attr}" ]]; then
        error "'${code_block_attr}' ignored because there is no subsequent Rust code block"
        code_block_attr=''
      fi
      if [[ "${line}" == '<!-- tidy:sync-markdown-to-rustdoc:code-block:'*' -->'* ]]; then
        code_block_attr="${line}"
        continue
      fi
      if [[ "${line}" == '<!-- tidy:sync-markdown-to-rustdoc:ignore:start -->'* ]]; then
        if [[ "${new}" == *$'\a\a' ]]; then
          new="${new%$'\a'}"
        fi
        ignore=1
        continue
      fi
      new+="${line}"$'\a'
    done < <(tr '\n' '\a' <"${markdown}" | grep -Eo '<!-- tidy:sync-markdown-to-rustdoc:start[^ ]* -->.*<!-- tidy:sync-markdown-to-rustdoc:end -->')
    new+='<!-- tidy:sync-markdown-to-rustdoc:end -->'
    new=$(tr '\n' '\a' <"${lib}" | sed -E "s/<!-- tidy:sync-markdown-to-rustdoc:start[^ ]* -->.*<!-- tidy:sync-markdown-to-rustdoc:end -->/$(sed_rhs_escape "${new}")/" | tr '\a' '\n')
    printf '%s\n' "${new}" >|"${lib}"
    check_diff "${lib}"
  done
  printf '\n'
else
  check_unused "Rust code" '*.cargo*' '*clippy.toml' '*deny.toml' '*rustfmt.toml' '*Cargo.toml' '*Cargo.lock'
fi
check_hidden clippy.toml deny.toml rustfmt.toml

# C/C++/Protobuf (if exists)
clang_format_ext=('*.c' '*.h' '*.cpp' '*.hpp' '*.proto')
if [[ -n "$(ls_files "${clang_format_ext[@]}")" ]]; then
  info "checking C/C++/Protobuf code style"
  check_config .clang-format
  if check_install clang-format; then
    IFS=' '
    info "running \`clang-format -i \$(git ls-files ${clang_format_ext[*]})\`"
    IFS=$'\n\t'
    clang-format -i $(ls_files "${clang_format_ext[@]}")
    check_diff $(ls_files "${clang_format_ext[@]}")
  fi
  printf '\n'
else
  check_unused "C/C++/Protobuf code" '*.clang-format*'
fi
check_alt '.clang-format' '_clang-format' "$(ls_files '*_clang-format')"
# https://gcc.gnu.org/onlinedocs/gcc/Overall-Options.html
check_alt '.cpp extension' 'other extensions' "$(ls_files '*.cc' '*.cp' '*.cxx' '*.C' '*.CPP' '*.c++')"
check_alt '.hpp extension' 'other extensions' "$(ls_files '*.hh' '*.hp' '*.hxx' '*.H' '*.HPP' '*.h++')"

# YAML/HTML/CSS/JavaScript/JSON (if exists)
prettier_ext=('*.css' '*.html' '*.js' '*.json' '*.yml' '*.yaml')
if [[ -n "$(ls_files "${prettier_ext[@]}")" ]]; then
  info "checking YAML/HTML/CSS/JavaScript/JSON code style"
  check_config .editorconfig
  if check_install npm; then
    IFS=' '
    info "running \`npx -y prettier -l -w \$(git ls-files ${prettier_ext[*]})\`"
    IFS=$'\n\t'
    npx -y prettier -l -w $(ls_files "${prettier_ext[@]}")
    check_diff $(ls_files "${prettier_ext[@]}")
  fi
  printf '\n'
else
  check_unused "YAML/HTML/CSS/JavaScript/JSON file" '*.prettierignore'
fi
# https://prettier.io/docs/en/configuration
check_alt '.editorconfig' 'other configs' "$(ls_files '*.prettierrc*' '*prettier.config.*')"
check_alt '.yml extension' '.yaml extension' "$(ls_files '*.yaml' | { grep -Fv '.markdownlint-cli2.yaml' || true; })"

# TOML (if exists)
if [[ -n "$(ls_files '*.toml' | { grep -Fv '.taplo.toml' || true; })" ]]; then
  info "checking TOML style"
  check_config .taplo.toml
  if check_install npm; then
    info "running \`npx -y @taplo/cli fmt \$(git ls-files '*.toml')\`"
    RUST_LOG=warn npx -y @taplo/cli fmt $(ls_files '*.toml')
    check_diff $(ls_files '*.toml')
  fi
  printf '\n'
else
  check_unused "TOML file" '*taplo.toml'
fi
check_hidden taplo.toml

# Markdown (if exists)
if [[ -n "$(ls_files '*.md')" ]]; then
  info "checking markdown style"
  check_config .markdownlint-cli2.yaml
  if check_install npm; then
    info "running \`npx -y markdownlint-cli2 \$(git ls-files '*.md')\`"
    if ! npx -y markdownlint-cli2 $(ls_files '*.md'); then
      error "check failed; please resolve the above markdownlint error(s)"
    fi
  fi
  printf '\n'
else
  check_unused "markdown file" '*.markdownlint-cli2.yaml'
fi
# https://github.com/DavidAnson/markdownlint-cli2#configuration
check_alt '.markdownlint-cli2.yaml' 'other configs' "$(ls_files '*.markdownlint-cli2.jsonc' '*.markdownlint-cli2.cjs' '*.markdownlint-cli2.mjs' '*.markdownlint.*')"
check_alt '.md extension' '*.markdown extension' "$(ls_files '*.markdown')"

# Shell scripts
info "checking shell scripts"
shell_files=()
docker_files=()
bash_files=()
grep_ere_files=()
sed_ere_files=()
for p in $(ls_files '*.sh' '*Dockerfile*'); do
  case "${p}" in
    tests/fixtures/* | */tests/fixtures/* | *.json) continue ;;
  esac
  case "${p##*/}" in
    *.sh)
      shell_files+=("${p}")
      re='^#!/.*bash'
      if [[ "$(head -1 "${p}")" =~ ${re} ]]; then
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
    if [[ "${p##*/}" == 'action.yml' ]]; then
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
  print_fenced "${res}"$'\n'
fi
# TODO: chmod|chown
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(basename|cat|cd|cp|dirname|ln|ls|mkdir|mv|pushd|rm|rmdir|tee|touch|kill|trap)( +-[0-9A-Za-z]+)* +[^<>\|-]' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
  error "use \`--\` before path(s): see https://github.com/koalaman/shellcheck/issues/2707 / https://github.com/koalaman/shellcheck/issues/2612 / https://github.com/koalaman/shellcheck/issues/2305 / https://github.com/koalaman/shellcheck/issues/2157 / https://github.com/koalaman/shellcheck/issues/2121 / https://github.com/koalaman/shellcheck/issues/314 for more"
  print_fenced "${res}"$'\n'
fi
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(LINES|RANDOM|PWD)=' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
  error "do not modify these built-in bash variables: see https://github.com/koalaman/shellcheck/issues/2160 / https://github.com/koalaman/shellcheck/issues/2559 for more"
  print_fenced "${res}"$'\n'
fi
# perf
res=$({ grep -En '(^|[^\\])\$\((cat) ' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
  error "use faster \`\$(<file)\` instead of \$(cat -- file): see https://github.com/koalaman/shellcheck/issues/2493 for more"
  print_fenced "${res}"$'\n'
fi
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(command +-[vV]) ' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
  error "use faster \`type -P\` instead of \`command -v\`: see https://github.com/koalaman/shellcheck/issues/1162 for more"
  print_fenced "${res}"$'\n'
fi
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(type) +-P +[^ ]+ +&>' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
  error "\`type -P\` doesn't output to stderr; use \`>\` instead of \`&>\`"
  print_fenced "${res}"$'\n'
fi
# TODO: multi-line case
res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(echo|printf )[^;)]* \|[^\|]' "${bash_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
if [[ -n "${res}" ]]; then
  error "use faster \`<<<...\` instead of \`echo ... |\`/\`printf ... |\`: see https://github.com/koalaman/shellcheck/issues/2593 for more"
  print_fenced "${res}"$'\n'
fi
# style
if [[ ${#grep_ere_files[@]} -gt 0 ]]; then
  # We intentionally do not check for occurrences in any other order (e.g., -iE, -i -E) here.
  # This enforces the style and makes it easier to search.
  res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(grep) +([^-]|-[^EFP-]|--[^hv])' "${grep_ere_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
  if [[ -n "${res}" ]]; then
    error "please always use ERE (grep -E) instead of BRE for code consistency within a file"
    print_fenced "${res}"$'\n'
  fi
fi
if [[ ${#sed_ere_files[@]} -gt 0 ]]; then
  res=$({ grep -En '(^|[^0-9A-Za-z\."'\''-])(sed) +([^-]|-[^E-]|--[^hv])' "${sed_ere_files[@]}" || true; } | { grep -Ev '^[^ ]+: *(#|//)' || true; } | LC_ALL=C sort)
  if [[ -n "${res}" ]]; then
    error "please always use ERE (sed -E) instead of BRE for code consistency within a file"
    print_fenced "${res}"$'\n'
  fi
fi
if check_install shfmt; then
  check_config .editorconfig
  info "running \`shfmt -w \$(git ls-files '*.sh')\`"
  if ! shfmt -w "${shell_files[@]}"; then
    error "check failed; please resolve the shfmt error(s)"
  fi
  check_diff "${shell_files[@]}"
fi
if [[ "${ostype}" == 'solaris' ]] && [[ -n "${CI:-}" ]] && ! type -P shellcheck >/dev/null; then
  warn "this check is skipped on Solaris due to no haskell/shellcheck in upstream package manager"
elif check_install shellcheck; then
  check_config .shellcheckrc
  info "running \`shellcheck \$(git ls-files '*.sh')\`"
  if ! shellcheck "${shell_files[@]}"; then
    error "check failed; please resolve the above shellcheck error(s)"
  fi
  # Check scripts in dockerfile.
  if [[ ${#docker_files[@]} -gt 0 ]]; then
    # Exclude SC2096 due to the way the temporary script is created.
    shellcheck_exclude=SC2096
    info "running \`shellcheck --exclude ${shellcheck_exclude}\` for scripts in \`\$(git ls-files '*Dockerfile*')\`"
    if check_install jq python3 parse-dockerfile; then
      shellcheck_for_dockerfile() {
        local text=$1
        local shell=$2
        local display_path=$3
        if [[ "${text}" == 'null' ]]; then
          return
        fi
        text="#!${shell}"$'\n'"${text}"
        case "${ostype}" in
          windows) text=${text//$'\r'/} ;; # Parse error on git bash/msys2 bash.
        esac
        local color=auto
        if [[ -t 1 ]] || [[ -n "${GITHUB_ACTIONS:-}" ]]; then
          color=always
        fi
        # We don't use <(printf '%s\n' "${text}") here because:
        # Windows: failed to found fd created by <() ("/proc/*/fd/* (git bash/msys2 bash) /dev/fd/* (cygwin bash): openBinaryFile: does not exist (No such file or directory)" error)
        # DragonFly BSD: hang
        # Others: false negative
        trap -- 'rm -- ./tools/.tidy-tmp; printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
        printf '%s\n' "${text}" >|./tools/.tidy-tmp
        if ! shellcheck --color="${color}" --exclude "${shellcheck_exclude}" ./tools/.tidy-tmp | sed -E "s/\.\/tools\/\.tidy-tmp/$(sed_rhs_escape "${display_path}")/g"; then
          error "check failed; please resolve the above shellcheck error(s)"
        fi
        rm -- ./tools/.tidy-tmp
        trap -- 'printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
      }
      for dockerfile_path in ${docker_files[@]+"${docker_files[@]}"}; do
        dockerfile=$(parse-dockerfile "${dockerfile_path}")
        normal_shell=''
        for instruction in $(jq -c '.instructions[]' <<<"${dockerfile}"); do
          instruction_kind=$(jq -r '.kind' <<<"${instruction}")
          case "${instruction_kind}" in
            FROM)
              # https://docs.docker.com/reference/dockerfile/#from
              # > Each FROM instruction clears any state created by previous instructions.
              normal_shell=''
              continue
              ;;
            ADD | ARG | CMD | COPY | ENTRYPOINT | ENV | EXPOSE | HEALTHCHECK | LABEL) ;;
            # https://docs.docker.com/reference/build-checks/maintainer-deprecated/
            MAINTAINER) error "MAINTAINER instruction is deprecated in favor of using label" ;;
            RUN) ;;
            SHELL)
              normal_shell=''
              for argument in $(jq -c '.arguments[]' <<<"${instruction}"); do
                value=$(jq -r '.value' <<<"${argument}")
                if [[ -z "${normal_shell}" ]]; then
                  case "${value}" in
                    cmd | cmd.exe | powershell | powershell.exe)
                      # not unix shell
                      normal_shell="${value}"
                      break
                      ;;
                  esac
                else
                  normal_shell+=' '
                fi
                normal_shell+="${value}"
              done
              ;;
            STOPSIGNAL | USER | VOLUME | WORKDIR) ;;
            *) error "unknown instruction ${instruction_kind}" ;;
          esac
          arguments=''
          # only shell-form RUN/ENTRYPOINT/CMD is run in a shell
          case "${instruction_kind}" in
            RUN)
              if [[ "$(jq -r '.arguments.shell' <<<"${instruction}")" == 'null' ]]; then
                continue
              fi
              arguments=$(jq -r '.arguments.shell.value' <<<"${instruction}")
              if [[ -z "${arguments}" ]]; then
                if [[ "$(jq -r '.here_docs[0]' <<<"${instruction}")" == 'null' ]]; then
                  error "empty RUN is useless (${dockerfile_path})"
                  continue
                fi
                if [[ "$(jq -r '.here_docs[1]' <<<"${instruction}")" != 'null' ]]; then
                  # TODO:
                  error "multi here-docs without command is not yet supported (${dockerfile_path})"
                fi
                arguments=$(jq -r '.here_docs[0].value' <<<"${instruction}")
                if [[ "${arguments}" == '#!'* ]]; then
                  # TODO:
                  error "here-docs with shebang is not yet supported (${dockerfile_path})"
                  continue
                fi
              else
                if [[ "$(jq -r '.here_docs[0]' <<<"${instruction}")" != 'null' ]]; then
                  # TODO:
                  error "sh/bash command with here-docs is not yet checked (${dockerfile_path})"
                fi
              fi
              ;;
            ENTRYPOINT | CMD)
              if [[ "$(jq -r '.arguments.shell' <<<"${instruction}")" == 'null' ]]; then
                continue
              fi
              arguments=$(jq -r '.arguments.shell.value' <<<"${instruction}")
              if [[ -z "${normal_shell}" ]] && [[ -n "${arguments}" ]]; then
                # https://docs.docker.com/reference/build-checks/json-args-recommended/
                error "JSON arguments recommended for ENTRYPOINT/CMD to prevent unintended behavior related to OS signals"
              fi
              ;;
            HEALTHCHECK)
              if [[ "$(jq -r '.arguments.kind' <<<"${instruction}")" != "CMD" ]]; then
                continue
              fi
              if [[ "$(jq -r '.arguments.arguments.shell' <<<"${instruction}")" == 'null' ]]; then
                continue
              fi
              arguments=$(jq -r '.arguments.arguments.shell.value' <<<"${instruction}")
              ;;
            *) continue ;;
          esac
          case "${normal_shell}" in
            # not unix shell
            cmd | cmd.exe | powershell | powershell.exe) continue ;;
            # https://docs.docker.com/reference/dockerfile/#shell
            '') shell='/bin/sh -c' ;;
            *) shell="${normal_shell}" ;;
          esac
          shellcheck_for_dockerfile "${arguments}" "${shell}" "${dockerfile_path}"
        done
      done
    fi
  fi
  # Check scripts in YAML.
  if [[ ${#workflows[@]} -gt 0 ]] || [[ ${#actions[@]} -gt 0 ]]; then
    # Exclude SC2096 due to the way the temporary script is created.
    shellcheck_exclude=SC2086,SC2096,SC2129
    info "running \`shellcheck --exclude ${shellcheck_exclude}\` for scripts in .github/workflows/*.yml and **/action.yml"
    if check_install jq python3 pipx; then
      shellcheck_for_gha() {
        local text=$1
        local shell=$2
        local display_path=$3
        if [[ "${text}" == 'null' ]]; then
          return
        fi
        case "${shell}" in
          bash* | sh*) ;;
          *) return ;;
        esac
        text="#!/usr/bin/env ${shell%' {0}'}"$'\n'"${text}"
        # Use python because sed doesn't support .*?.
        text=$(
          "python${py_suffix}" - <<EOF
import re
text = re.sub(r"\\\${{.*?}}", "\${__GHA_SYNTAX__}", r'''${text}''')
print(text)
EOF
        )
        case "${ostype}" in
          windows) text=${text//$'\r'/} ;; # Python print emits \r\n.
        esac
        local color=auto
        if [[ -t 1 ]] || [[ -n "${GITHUB_ACTIONS:-}" ]]; then
          color=always
        fi
        # We don't use <(printf '%s\n' "${text}") here because:
        # Windows: failed to found fd created by <() ("/proc/*/fd/* (git bash/msys2 bash) /dev/fd/* (cygwin bash): openBinaryFile: does not exist (No such file or directory)" error)
        # DragonFly BSD: hang
        # Others: false negative
        trap -- 'rm -- ./tools/.tidy-tmp; printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
        printf '%s\n' "${text}" >|./tools/.tidy-tmp
        if ! shellcheck --color="${color}" --exclude "${shellcheck_exclude}" ./tools/.tidy-tmp | sed -E "s/\.\/tools\/\.tidy-tmp/$(sed_rhs_escape "${display_path}")/g"; then
          error "check failed; please resolve the above shellcheck error(s)"
        fi
        rm -- ./tools/.tidy-tmp
        trap -- 'printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
      }
      for workflow_path in ${workflows[@]+"${workflows[@]}"}; do
        workflow=$(yq -c '.' "${workflow_path}")
        # The top-level permissions must be weak as they are referenced by all jobs.
        permissions=$(jq -c '.permissions' <<<"${workflow}")
        case "${permissions}" in
          # `permissions: {}` means "all none": https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#defining-access-for-the-github_token-scopes
          '{"contents":"read"}' | '{}') ;;
          null) error "${workflow_path}: top level permissions not found; it must be 'contents: read' or weaker permissions" ;;
          *) error "${workflow_path}: only 'contents: read' and weaker permissions are allowed at top level, but found '${permissions}'; if you want to use stronger permissions, please set job-level permissions" ;;
        esac
        default_shell=$(jq -r -c '.defaults.run.shell' <<<"${workflow}")
        # github's default is https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#defaultsrunshell
        re='^bash --noprofile --norc -CeEux?o pipefail \{0}$'
        if [[ ! "${default_shell}" =~ ${re} ]]; then
          error "${workflow_path}: defaults.run.shell should be 'bash --noprofile --norc -CeEuxo pipefail {0}' or 'bash --noprofile --norc -CeEuo pipefail {0}'"
          continue
        fi
        # .steps == null means the job is the caller of reusable workflow
        for job in $(jq -c '.jobs | to_entries[] | select(.value.steps)' <<<"${workflow}"); do
          name=$(jq -r '.key' <<<"${job}")
          job=$(jq -r '.value' <<<"${job}")
          n=0
          job_default_shell=$(jq -r '.defaults.run.shell' <<<"${job}")
          if [[ "${job_default_shell}" == 'null' ]]; then
            job_default_shell="${default_shell}"
          fi
          for step in $(jq -c '.steps[]' <<<"${job}"); do
            prepare=''
            eval "$(jq -r 'if .run then @sh "RUN=\(.run) shell=\(.shell)" else @sh "RUN=\(.with.run) prepare=\(.with.prepare) shell=\(.with.shell)" end' <<<"${step}")"
            if [[ "${RUN}" == 'null' ]]; then
              _=$((n++))
              continue
            fi
            if [[ "${shell}" == 'null' ]]; then
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
          if [[ "${RUN}" == 'null' ]]; then
            _=$((n++))
            continue
          fi
          if [[ "${shell}" == 'null' ]]; then
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
zizmor_targets=(${workflows[@]+"${workflows[@]}"} ${actions[@]+"${actions[@]}"})
if [[ -e .github/dependabot.yml ]]; then
  zizmor_targets+=(.github/dependabot.yml)
fi
if [[ ${#zizmor_targets[@]} -gt 0 ]]; then
  if [[ "${ostype}" =~ ^(netbsd|openbsd|dragonfly|illumos|solaris)$ ]] && [[ -n "${CI:-}" ]] && ! type -P zizmor >/dev/null; then
    warn "this check is skipped on NetBSD/OpenBSD/Dragonfly/illumos/Solaris due to installing zizmor is hard on these platform"
  elif check_install zizmor; then
    IFS=' '
    info "running \`zizmor -q ${zizmor_targets[*]}\`"
    IFS=$'\n\t'
    zizmor -q "${zizmor_targets[@]}"
  fi
fi
printf '\n'
check_alt '.sh extension' '*.bash extension' "$(ls_files '*.bash')"

# License check
# TODO: This check is still experimental and does not track all files that should be tracked.
if [[ -f tools/.tidy-check-license-headers ]]; then
  info "checking license headers (experimental)"
  failed_files=''
  for p in $(LC_ALL=C comm -12 <(eval $(<tools/.tidy-check-license-headers) | LC_ALL=C sort) <(ls_files | LC_ALL=C sort)); do
    case "${p##*/}" in
      *.stderr | *.expanded.rs) continue ;; # generated files
      *.json) continue ;;                   # no comment support
      *.sh | *.py | *.rb | *Dockerfile*) prefix=('# ') ;;
      *.rs | *.c | *.h | *.cpp | *.hpp | *.s | *.S | *.js) prefix=('// ' '/* ') ;;
      *.ld | *.x) prefix=('/* ') ;;
      # TODO: More file types?
      *) continue ;;
    esac
    # TODO: The exact line number is not actually important; it is important
    # that it be part of the top-level comments of the file.
    line=1
    if IFS= LC_ALL=C read -rd '' -n3 shebang <"${p}" && [[ "${shebang}" == '#!/' ]]; then
      line=2
    elif [[ "${p}" == *'Dockerfile'* ]] && IFS= LC_ALL=C read -rd '' -n9 syntax <"${p}" && [[ "${syntax}" == '# syntax=' ]]; then
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
    print_fenced "${failed_files}"
  else
    printf '\n'
  fi
fi

# Spell check (if config exists)
if [[ -f .cspell.json ]]; then
  info "spell checking"
  project_dictionary=.github/.cspell/project-dictionary.txt
  if check_install npm jq python3 pipx; then
    has_rust=''
    if [[ -n "$(ls_files '*Cargo.toml')" ]]; then
      has_rust=1
      dependencies=''
      for manifest_path in $(ls_files '*Cargo.toml'); do
        if [[ "${manifest_path}" != "Cargo.toml" ]] && [[ "$(tomlq -c '.workspace' "${manifest_path}")" == 'null' ]]; then
          continue
        fi
        m=$(cargo metadata --format-version=1 --no-deps --manifest-path "${manifest_path}" || true)
        if [[ -z "${m}" ]]; then
          continue # Ignore broken manifest
        fi
        dependencies+="$(jq -r '. as $metadata | .workspace_members[] as $id | $metadata.packages[] | select(.id == $id) | .dependencies[].name' <<<"${m}")"$'\n'
      done
      dependencies=$(LC_ALL=C sort -f -u <<<"${dependencies//[0-9_-]/$'\n'}")
    fi
    config_old=$(<.cspell.json)
    config_new=$({ grep -Ev '^ *//' <<<"${config_old}" || true; } | jq 'del(.dictionaries[] | select(index("organization-dictionary") | not)) | del(.dictionaryDefinitions[] | select(.name == "organization-dictionary" | not))')
    trap -- 'printf "%s\n" "${config_old}" >|.cspell.json; printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
    printf '%s\n' "${config_new}" >|.cspell.json
    dependencies_words=''
    if [[ -n "${has_rust}" ]]; then
      dependencies_words=$(npx -y cspell stdin --no-progress --no-summary --words-only --unique <<<"${dependencies}" || true)
    fi
    all_words=$(ls_files | { grep -Fv "${project_dictionary}" || true; } | npx -y cspell --file-list stdin --no-progress --no-summary --words-only --unique || true)
    all_words+=$'\n'$(ls_files | npx -y cspell stdin --no-progress --no-summary --words-only --unique || true)
    printf '%s\n' "${config_old}" >|.cspell.json
    trap -- 'printf >&2 "%s\n" "${0##*/}: trapped SIGINT"; exit 1' SIGINT
    cat >|.github/.cspell/rust-dependencies.txt <<EOF
# This file is @generated by ${0##*/}.
# It is not intended for manual editing.
EOF
    if [[ -n "${dependencies_words}" ]]; then
      LC_ALL=C sort -f >>.github/.cspell/rust-dependencies.txt <<<"${dependencies_words}"$'\n'
    fi
    if [[ -z "${CI:-}" ]]; then
      REMOVE_UNUSED_WORDS=1
    fi
    if [[ -z "${REMOVE_UNUSED_WORDS:-}" ]]; then
      check_diff .github/.cspell/rust-dependencies.txt
    fi
    if ! grep -Fq '.github/.cspell/rust-dependencies.txt linguist-generated' .gitattributes; then
      error "you may want to mark .github/.cspell/rust-dependencies.txt linguist-generated"
    fi

    # Check file names.
    info "running \`git ls-files | npx -y cspell stdin --no-progress --no-summary --show-context\`"
    if ! ls_files | npx -y cspell stdin --no-progress --no-summary --show-context; then
      error "spellcheck failed: please fix uses of below words in file names or add to ${project_dictionary} if correct"
      printf '=======================================\n'
      { ls_files | npx -y cspell stdin --no-progress --no-summary --words-only || true; } | sed -E "s/'s$//g" | LC_ALL=C sort -f -u
      printf '=======================================\n\n'
    fi
    # Check file contains.
    info "running \`git ls-files | npx -y cspell --file-list stdin --no-progress --no-summary\`"
    if ! ls_files | npx -y cspell --file-list stdin --no-progress --no-summary; then
      error "spellcheck failed: please fix uses of below words or add to ${project_dictionary} if correct"
      printf '=======================================\n'
      { ls_files | npx -y cspell --file-list stdin --no-progress --no-summary --words-only || true; } | sed -E "s/'s$//g" | LC_ALL=C sort -f -u
      printf '=======================================\n\n'
    fi

    # Make sure the project-specific dictionary does not contain duplicated words.
    for dictionary in .github/.cspell/*.txt; do
      if [[ "${dictionary}" == "${project_dictionary}" ]]; then
        continue
      fi
      case "${ostype}" in
        # NetBSD uniq doesn't support -i flag.
        netbsd) dup=$(sed -E 's/#.*//g; s/^[ \t]+//g; s/\/[ \t]+$//g; /^$/d' "${project_dictionary}" "${dictionary}" | LC_ALL=C sort -f | tr '[:upper:]' '[:lower:]' | LC_ALL=C uniq -d) ;;
        *) dup=$(sed -E 's/#.*//g; s/^[ \t]+//g; s/\/[ \t]+$//g; /^$/d' "${project_dictionary}" "${dictionary}" | LC_ALL=C sort -f | LC_ALL=C uniq -d -i) ;;
      esac
      if [[ -n "${dup}" ]]; then
        error "duplicated words in dictionaries; please remove the following words from ${project_dictionary}"
        print_fenced "${dup}"$'\n'
      fi
    done

    # Make sure the project-specific dictionary does not contain unused words.
    if [[ -n "${REMOVE_UNUSED_WORDS:-}" ]]; then
      grep_args=()
      while IFS= read -r word; do
        if ! grep -Eqi "^${word}$" <<<"${all_words}"; then
          grep_args+=(-e "^[ \t]*${word}[ \t]*(#.*|$)")
        fi
      done < <(sed -E 's/#.*//g; s/^[ \t]+//g; s/\/[ \t]+$//g; /^$/d' "${project_dictionary}")
      if [[ ${#grep_args[@]} -gt 0 ]]; then
        info "removing unused words from ${project_dictionary}"
        info "please commit changes made by the removal above"
        res=$(grep -Ev "${grep_args[@]}" "${project_dictionary}" || true)
        if [[ -n "${res}" ]]; then
          printf '%s\n' "${res}" >|"${project_dictionary}"
        else
          printf '' >|"${project_dictionary}"
        fi
      fi
    else
      unused=''
      while IFS= read -r word; do
        if ! grep -Eqi "^${word}$" <<<"${all_words}"; then
          unused+="${word}"$'\n'
        fi
      done < <(sed -E 's/#.*//g; s/^[ \t]+//g; s/\/[ \t]+$//g; /^$/d' "${project_dictionary}")
      if [[ -n "${unused}" ]]; then
        error "unused words in dictionaries; please remove the following words from ${project_dictionary} or run ${0##*/} locally"
        print_fenced "${unused}"
      fi
    fi
  fi
  printf '\n'
fi

if [[ -n "${should_fail:-}" ]]; then
  exit 1
fi
