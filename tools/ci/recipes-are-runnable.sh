#!/bin/sh
# The CI recipes this repository ships run commands uf has, in an order and on
# an image that can run them.
#
# `tools/ci/integrations-agree.sh` checks the *installation*: that the three
# integrations configure `install.sh` with variables it reads, so `uf` ends up
# on `PATH`. This checks what happens next — the recipes themselves. They are
# YAML for three systems this repository has no runner for, and until this
# existed nothing at all read them:
#
#   integrations/github-actions/action.yml
#   integrations/gitlab/uf.gitlab-ci.yml
#   integrations/circleci/orb.yml
#   docs/app/guide/ci/$page.mdx     (its fenced blocks, which are copied)
#
# That is the whole argument for this file. A shipped recipe is a copy of the
# CLI's surface that ages on its own: rename a flag and three files go on
# passing it, in pipelines belonging to people who cannot fix them. So the
# recipes are read back against the binary rather than against a memory of it —
# every command below is asked of `uf <command> --help` at the moment the check
# runs, which is the only source that cannot be stale.
#
# Five rules. Three of them were failing when this was written, and the
# integrations were fixed in the same commit; the failures are described where
# each rule is.
#
#   1. Every `uf` command a recipe runs names a subcommand the binary has, and
#      passes only flags that subcommand accepts.
#   2. Every one of those commands can be read by a machine: it offers `--json`,
#      or it is named in NO_MACHINE_READABLE_FORM below with the reason. A CI
#      job should fail on a report, not on a grep of prose.
#   3. A command that reads `node_modules` is preceded, in the same recipe, by
#      `uf install`. This is the rule worth having: `uf check` with nothing
#      installed does not fail, it *passes* — every import that resolves to
#      nothing is typed `any` — so the job goes green having checked a program
#      it could not see.
#   4. A recipe that names its own image names one with a JavaScript runtime and
#      a package manager on it, when it runs a command that needs one.
#   5. A cache key for the toolchain names both the version and the
#      architecture. A key missing either is a hit that can run nothing.
#
# It needs the binary. `UF_BIN` points at one — the `Metadata` job passes
# `./target/release/uf`, which is the artefact `Toolchain` built — and the check
# fails rather than guessing when there is none, because a rule that quietly
# stops being applied is the failure mode every gate in this directory exists to
# prevent.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

# --- The written lists -----------------------------------------------------

# Recipes that ship, and the guide whose fenced blocks people copy.
YAML_RECIPES="integrations/github-actions/action.yml
integrations/gitlab/uf.gitlab-ci.yml
integrations/circleci/orb.yml"
GUIDE='docs/app/guide/ci/$page.mdx'

# Commands that read `node_modules`, and therefore need `uf install` to have run.
#
#   check   resolves an import to a file in its batch or to a package under
#           `node_modules`; a specifier that answers to neither is typed `any`.
#           With nothing installed it reports "no problems" over a program it
#           could not see. It is the one that passes rather than failing, which
#           is why this rule exists.
#   fmt     prints Flow from the parser's tree and needs nothing for that, but
#           JSON, CSS and TypeScript go to a formatter uf runs rather than links
#           in, found in `node_modules/.bin`. With none there those files are
#           reported skipped and deliberately do not reach the exit code, so the
#           job passes over files nothing formatted.
#   build   runs Vite, which is a dependency.
#   test    runs the suite on a Capability JS Host against the project's own
#           imports.
NEEDS_DEPENDENCIES="build check fmt test"

# Commands that need a JavaScript runtime or a package manager to exist on the
# image at all — as opposed to needing the project installed. `install` is here
# and not above: it *is* the install, and it drives npm, pnpm, yarn or bun.
NEEDS_A_JS_HOST="build dev install preview start test"

# Images that carry one, as prefixes. This list is short on purpose: it governs
# the images *this repository* writes into its own recipes, not what a project
# may pass to `uf/run`. Add to it when a recipe here needs another, and say what
# the image provides.
#
#   node:       Debian with Node and npm, which is the whole requirement.
#   cimg/node:  CircleCI's convenience image, Node and npm plus their caches.
IMAGES_WITH_A_JS_HOST="node: cimg/node:"

# Commands a recipe runs whose result a job reads without `--json`, with the
# reason. Rule 2 requires `--json` of everything else.
#
#   install  answers with the tree it wrote and its exit code; `--frozen-lockfile`
#            makes a drifted lockfile the failure, which is the outcome a
#            pipeline is asking about.
#   build    answers with its exit code, and writes a build manifest into the
#            output directory — a file, which is machine-readable in the way
#            that matters for a build.
#   fmt      answers with its exit code and a list of files on stdout, and has
#            no `--json` yet. This is a gap rather than a design: `uf check`,
#            `uf lint` and `uf test` all have one, and a job that wants to know
#            *which* files need formatting has to read prose. Named here so the
#            exemption is a decision somebody made rather than one nobody
#            noticed.
NO_MACHINE_READABLE_FORM="build fmt install"

# --- The binary ------------------------------------------------------------

uf="${UF_BIN:-}"
if [ -z "$uf" ]; then
  for candidate in target/release/uf target/debug/uf; do
    if [ -x "$candidate" ]; then
      uf="$candidate"
      break
    fi
  done
fi
if [ -z "$uf" ] || [ ! -x "$uf" ]; then
  echo "recipes: FAIL: no uf binary to ask." >&2
  echo "  This check reads the recipes back against the CLI rather than against" >&2
  echo "  a copy of it, so it cannot run without one. Build it, or point at it:" >&2
  echo "    cargo build --release --bin uf" >&2
  echo "    UF_BIN=/path/to/uf tools/ci/recipes-are-runnable.sh" >&2
  exit 1
fi

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-recipes.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

status=0

fail() {
  echo "recipes: FAIL: $*" >&2
  status=1
}

# `uf <name> --help`, cached, or empty when there is no such command.
help_for() {
  cached="$work/help.$1"
  if [ ! -f "$cached" ]; then
    if "$uf" "$1" --help >"$cached" 2>/dev/null; then
      :
    else
      : >"$cached"
    fi
  fi
  cat "$cached"
}

in_list() {
  in_list_needle="$1"
  for in_list_item in $2; do
    [ "$in_list_item" = "$in_list_needle" ] && return 0
  done
  return 1
}

# --- Reading the recipes ---------------------------------------------------
#
# One record per `uf` command, as `file|line|unit|kind|command`.
#
# A *unit* is one recipe: a top-level key in a YAML file (`uf:check:`, `jobs:`)
# and a fenced block in the guide. Rule 3 is about order within a unit, and a
# `uf install` three jobs away is not an install this one ran.
#
# A *step* is a line a shell will run — `- uf check`, `- run: uf build` — and a
# *declaration* is a command written as a value, which is what a CircleCI
# parameter default and an orb invocation's `command:` are. Both are checked for
# being real commands; only steps are ordered, because a default is not a thing
# that runs before or after anything.
#
# Line-oriented, and it knows the shape these four files are written in, for the
# same reason `gate-covers-every-job.sh` is: this runs in the `Metadata` job,
# which installs nothing. Prose is deliberately out of reach — every `uf` here
# has to sit in a command position, so the sentence "uf publishes macOS and
# Linux binaries only" in an action's description is not read as a subcommand
# called `publishes`.
extract() {
  awk -v MODE="$2" '
    BEGIN { unit = "(top)"; fence = 0; infence = 0 }
    MODE == "mdx" {
      if ($0 ~ /^[ \t]*```/) {
        infence = 1 - infence
        if (infence) { fence++; unit = "block " fence }
        next
      }
      if (!infence) next
    }
    {
      raw = $0
      if (MODE == "yaml" && raw ~ /^[^ \t#]/) {
        u = raw
        if (sub(/:[ \t]*$/, "", u)) unit = u
      }
      if (raw ~ /^[ \t]*#/) next
      line = raw
      sub(/[ \t]+#.*$/, "", line)
      kind = ""
      if (sub(/^[ \t]*-[ \t]+run:[ \t]*/, "", line)) kind = "step"
      else if (sub(/^[ \t]*-[ \t]+/, "", line)) kind = "step"
      else if (sub(/^[ \t]*run:[ \t]*/, "", line)) kind = "step"
      else if (sub(/^[ \t]*command:[ \t]*/, "", line)) kind = "decl"
      else if (sub(/^[ \t]*default:[ \t]*/, "", line)) kind = "decl"
      else next
      if (line !~ /^uf[ \t]+[a-z]/) next
      gsub(/^[ \t]+|[ \t]+$/, "", line)
      printf "%s|%d|%s|%s|%s\n", FILENAME, NR, unit, kind, line
    }
  ' "$1"
}

for file in $YAML_RECIPES $GUIDE; do
  [ -f "$file" ] || {
    fail "$file is missing"
    continue
  }
done
[ "$status" -eq 0 ] || exit 1

: >"$work/records"
for file in $YAML_RECIPES; do
  extract "$file" yaml >>"$work/records"
done
extract "$GUIDE" mdx >>"$work/records"

if [ ! -s "$work/records" ]; then
  fail "found no uf command in any recipe — have they been rewritten?"
  exit 1
fi

# --- Rules 1, 2 and 3 ------------------------------------------------------

subcommands=""
# The last `uf install` seen, per unit. Reset when the unit changes: the records
# are in file order, and a unit's lines are contiguous.
current_unit=""
installed_at=""

while IFS='|' read -r file line unit kind command; do
  [ -n "$command" ] || continue
  if [ "$file|$unit" != "$current_unit" ]; then
    current_unit="$file|$unit"
    installed_at=""
  fi

  # shellcheck disable=SC2086
  set -- $command
  shift # `uf`
  name="$1"
  shift

  # 1. The command exists.
  help="$(help_for "$name")"
  if [ -z "$help" ]; then
    fail "$file:$line runs \`uf $name\`, which this uf has no command for"
    continue
  fi

  # 1. And so does every flag it passes.
  flag_trouble=""
  for word in "$@"; do
    case "$word" in
      --*)
        # `--flag=value` names the flag before the `=`.
        flag="${word%%=*}"
        if ! printf '%s\n' "$help" |
          grep -qE -- "(^|[^A-Za-z0-9_-])$flag([^A-Za-z0-9_-]|$)"; then
          fail "$file:$line passes $flag to \`uf $name\`, which does not accept it"
          flag_trouble="yes"
        fi
        ;;
      *) ;;
    esac
  done
  [ -z "$flag_trouble" ] || continue

  case " $subcommands " in
    *" $name "*) ;;
    *) subcommands="$subcommands $name" ;;
  esac

  # 3. Order, for the commands that read `node_modules`.
  if [ "$kind" = "step" ]; then
    if [ "$name" = "install" ]; then
      installed_at="$line"
    elif in_list "$name" "$NEEDS_DEPENDENCIES"; then
      if [ -z "$installed_at" ]; then
        fail "$file:$line runs \`uf $name\` with no \`uf install\` before it in \`$unit\`"
        echo "        \`uf $name\` reads node_modules, and installing the toolchain is" >&2
        echo "        not installing the project. The two that do not simply fail are" >&2
        echo "        the reason this is checked: \`uf check\` types every unresolved" >&2
        echo "        import as \`any\` and reports no problems, and \`uf fmt --check\`" >&2
        echo "        skips the files whose formatter lives in node_modules/.bin" >&2
        echo "        without letting them reach the exit code. Both go green." >&2
      fi
    fi
  fi
done <"$work/records"

echo "commands the recipes run:$subcommands"

# 2. Each of them can be read by a machine.
for name in $subcommands; do
  if in_list "$name" "$NO_MACHINE_READABLE_FORM"; then
    echo "  --  uf $name answers with its exit code (see NO_MACHINE_READABLE_FORM)"
    continue
  fi
  if help_for "$name" | grep -qE -- '(^|[^A-Za-z0-9_-])--json([^A-Za-z0-9_-]|$)'; then
    echo "  ok  uf $name --json"
  else
    fail "a recipe runs \`uf $name\`, which offers no --json"
    echo "        A CI job has to read this command's result by grepping prose." >&2
    echo "        Give it --json, or name it in NO_MACHINE_READABLE_FORM with the" >&2
    echo "        reason a job does not need one." >&2
  fi
done

# --- Rule 4: the image can run what the recipe runs -------------------------

for file in $YAML_RECIPES; do
  needs_host=""
  while IFS='|' read -r record_file _ _ _ command; do
    [ "$record_file" = "$file" ] || continue
    # shellcheck disable=SC2086
    set -- $command
    if in_list "$2" "$NEEDS_A_JS_HOST"; then
      needs_host="yes"
    fi
  done <"$work/records"
  [ -n "$needs_host" ] || continue

  # A whole line at a time: an image is one value, and `cimg/node:<< parameters.tag >>`
  # is three words.
  grep -v '^[[:space:]]*#' "$file" |
    sed -n 's/^[[:space:]]*-\{0,1\}[[:space:]]*image:[[:space:]]*//p' >"$work/images"
  while IFS= read -r image; do
    [ -n "$image" ] || continue
    known=""
    for prefix in $IMAGES_WITH_A_JS_HOST; do
      case "$image" in
        "$prefix"*) known="yes" ;;
      esac
    done
    if [ -n "$known" ]; then
      echo "  ok  $file runs on $image"
    else
      fail "$file runs a command needing a JavaScript host on \`$image\`"
      echo "        \`uf install\`, \`uf test\` and \`uf build\` need a runtime and a" >&2
      echo "        package manager on the image. Name one of:" >&2
      echo "        $IMAGES_WITH_A_JS_HOST" >&2
    fi
  done <"$work/images"
done

# --- Rule 5: a toolchain cache key names the version and the architecture ---
#
# A key that names neither is the worst of the three, and all three have been
# wrong at least once. Without the version a pipeline that pins one is handed
# another; without the architecture a fleet with amd64 and arm64 runners in it
# restores one target's binary onto the other and gets `Exec format error` out
# of a cache that reported a hit. Both are keys that "hit" and can run nothing,
# which is the failure each integration already guards against at run time by
# making the restored binary answer `uf --version` before believing it — this is
# the same rule, one step earlier.
VERSION_TOKENS="UF_VERSION inputs.version parameters.version"
ARCH_TOKENS="runner.arch runner.os CI_RUNNER_EXECUTABLE_ARCH {{ arch }}"

for file in $YAML_RECIPES; do
  keys="$(
    grep -v '^[[:space:]]*#' "$file" |
      grep -E '^[[:space:]]*-?[[:space:]]*keys?:' |
      grep -- 'uf-' || true
  )"
  [ -n "$keys" ] || continue
  printf '%s\n' "$keys" | while IFS= read -r key; do
    [ -n "$key" ] || continue
    # A dependency cache answers to the lockfile rather than to the toolchain,
    # and GitLab spells that as `key: files:` on the lines below rather than
    # inside the key. Only the toolchain's own key is this rule's business.
    case "$key" in
      *deps*) continue ;;
    esac
    has_version=""
    for token in $VERSION_TOKENS; do
      case "$key" in
        *"$token"*) has_version="yes" ;;
      esac
    done
    has_arch=""
    for token in $ARCH_TOKENS; do
      case "$key" in
        *"$token"*) has_arch="yes" ;;
      esac
    done
    if [ -z "$has_version" ]; then
      echo "recipes: FAIL: $file caches the toolchain under a key naming no version:" >&2
      echo "       $key" >&2
      exit 1
    fi
    if [ -z "$has_arch" ]; then
      echo "recipes: FAIL: $file caches the toolchain under a key naming no architecture:" >&2
      echo "       $key" >&2
      echo "        uf ships one archive per target. A cache shared by runners of" >&2
      echo "        more than one architecture will restore the wrong binary and" >&2
      echo "        report a hit. Name one of: $ARCH_TOKENS" >&2
      exit 1
    fi
    echo "  ok  $file caches the toolchain on the version and the architecture"
  done || status=1
done

if [ "$status" -eq 0 ]; then
  echo "recipes-are-runnable: ok"
fi
exit "$status"
