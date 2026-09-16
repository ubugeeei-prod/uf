#!/usr/bin/env sh
# Every job that runs the workspace test suite installs the runtimes it starts.
#
# `crates/uf_cli/tests/{bun_host,deno_host,permissions}.rs` start real Bun,
# Deno and Node processes, and `crates/uf_cli/tests/managers.rs` real pnpm and
# Yarn releases, and they **fail rather than skip** when one is absent. That is
# the policy working — a claim nobody checked must not be green — but it makes
# `cargo test --workspace` runnable only in a job that installed all of them.
#
# `publish.yml` was not such a job. `uf@0.0.0-alpha.14` was tagged, its four
# binaries were built and released, and the publish job failed on six Deno
# tests before publishing a single package: a tag, a GitHub release, and
# nothing on npm. Deno had been added to `ci.yml`'s suite-running jobs and not
# to the third one, which lives in another file.
#
# So the rule is checked rather than remembered, from the side that cannot be
# forgotten: find the jobs that run the suite — directly, or through
# `uf run rust:test` or `uf run ci`, which are the same command — and then look
# for each runtime's setup step. A workflow added later that runs the suite is
# caught the first time it does.
#
# Installing a runtime is half of it. The other half is an environment it can
# run in, and that is the second rule below. `publish.yml` configured npm's
# publishing registry before the suite, and `actions/setup-node`'s
# `registry-url:` writes an `.npmrc` holding `_authToken=${NODE_AUTH_TOKEN}`
# and points every npm client at it for the rest of the job. Yarn 1 expands
# environment variables in npm config eagerly, on every command, and exits when
# one is unset — so with the token in scope for the publish step alone, all
# seven Yarn 1 rows of `crates/uf_cli/tests/managers.rs` failed on
# `Failed to replace env in config: ${NODE_AUTH_TOKEN}` and
# `uf@0.0.0-alpha.38` sent no package to npm. `ci.yml`'s `Test` job runs the
# same suite and passes, because it configures no publishing registry — which
# is also the environment a contributor's machine has.
#
# So: a job that runs the suite under a registry-authenticated `.npmrc` has to
# have the token in scope where the suite runs, or configure the registry after
# the suite rather than before it.
#
# No temporary files: this runs in `Metadata`, which installs nothing, and on a
# machine whose `mktemp` may be refused.
set -eu

root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

# name : the action that installs it : the test file that starts it.
# A runtime whose test file does not exist is not required yet.
# A literal tab, taken once. Nesting `$(printf '\t')` inside the `$( … )`
# below is a parse error on bash 3.2, which is `/bin/sh` on macOS — and
# `scripts:parse` runs `sh -n` with whatever `sh` is, so a script that only
# parses under dash is a script that fails somebody's check.
TAB=$(printf '\t')

#
# pnpm and both Yarns are not an action but a script, which pins their
# releases in one place; a job that runs the script installs all four.
#
# The last row is not a runtime but has the same shape: the React Compiler
# conformance run fails rather than skips without its fixture checkout, so a
# job running the suite has to run the step that fetches it.
RUNTIMES="bun:oven-sh/setup-bun:crates/uf_cli/tests/bun_host.rs
deno:denoland/setup-deno:crates/uf_cli/tests/deno_host.rs
node:actions/setup-node:crates/uf_cli/tests/permissions.rs
package-managers:tools/ci/install-package-managers.sh:crates/uf_cli/tests/managers.rs
react-compiler-fixtures:tools/react-compiler/sync.sh:crates/uf_transform/tests/react_compiler_conformance/main.rs"

# One pass, emitting `RUNS <workflow> <job>` and
# `MISSING <workflow> <job> <runtime> <file>`, so the results survive the
# subshell a pipeline puts the loop in.
#
# It is a function rather than the body of the `$( … )` below, and that is not
# style: bash 3.2 — `/bin/sh` on macOS — cannot parse a `case` inside a command
# substitution, and `scripts:parse` runs `sh -n` with whatever `sh` is.
scan() {
  for workflow in .github/workflows/*.yml; do
    # One line per job: name, a tab, then its whole body flattened. A job starts
    # at two-space indentation under `jobs:`; everything more indented is part
    # of it.
    awk '
      /^jobs:/ { in_jobs = 1; next }
      !in_jobs { next }
      /^[^ ]/  { in_jobs = 0; next }
      /^  [A-Za-z0-9_-]+:/ {
        if (job != "") printf "%s\t%s\n", job, body
        job = $1; sub(/:$/, "", job); body = ""; next
      }
      # Only what the job *does*. A comment mentioning `uf run ci` is prose,
      # and reading it as a command is how this check first reported four jobs
      # that run no suite at all.
      /^ *#/ { next }
      # `deno-version:` too, because installing a runtime is half the rule; the
      # other half is installing the one the suite needs.
      /run:|uses:|deno-version:/ { body = body " " $0 }
      END { if (job != "") printf "%s\t%s\n", job, body }
    ' "$workflow" | while IFS="$TAB" read -r job body; do
      case "$body" in
        *"cargo test --workspace"* | *"uf run rust:test"*) ;;
        *"uf run ci "* | *"uf run ci") ;;
        *) continue ;;
      esac
      echo "RUNS $workflow $job"
      for runtime in $RUNTIMES; do
        name=${runtime%%:*}
        rest=${runtime#*:}
        action=${rest%%:*}
        needed_by=${rest#*:}
        [ -f "$needed_by" ] || continue
        case "$body" in
          *"$action"*) ;;
          *) echo "MISSING $workflow $job $name $needed_by" ;;
        esac
      done
      case "$body" in
        *"denoland/setup-deno"*)
          version=$(printf '%s\n' "$body" | sed -n 's/.*deno-version:[[:space:]]*\([^[:space:]]*\).*/\1/p')
          echo "DENO $workflow $job ${version:-default}"
          ;;
      esac
    done
  done
}

# The same workflows, read a second way: in step order, because what matters
# here is what is in effect *by the time* the suite runs rather than what the
# job contains somewhere.
#
# Each job becomes one line — `NPMRC <workflow> <job>` and then a mark per step
# that matters, in the order the steps run:
#
#   J  the job, or the workflow, defines NODE_AUTH_TOKEN for every step
#   R  this step configures a publishing registry, for itself and every step
#      after it
#   S  this step runs the workspace suite
#   T  this step defines NODE_AUTH_TOKEN for itself
#
# `R S` is the defect. `R ST` and `J R S` are the token remedies, `S R` the
# ordering one, and a `T` that arrives after the `S` — the token defined for
# the publish step, which is what `publish.yml` would have had — is no remedy
# at all, because the suite has already run by then.
scan_npmrc() {
  for workflow in .github/workflows/*.yml; do
    awk -v workflow="$workflow" '
      function flush_step() {
        if (in_step) {
          mark = ""
          if (step ~ /registry-url:|_authToken/) mark = mark "R"
          if (step ~ /cargo test --workspace/ || step ~ /uf run rust:test/ ||
              step ~ /uf run ci([[:space:]]|$)/) mark = mark "S"
          if (step ~ /NODE_AUTH_TOKEN[[:space:]]*[:=]/) mark = mark "T"
          if (mark != "") marks = marks " " mark
        }
        step = ""; in_step = 0
      }
      function flush_job() {
        flush_step()
        if (job != "")
          printf "NPMRC %s %s%s%s\n", workflow, job,
                 (job_token || file_token) ? " J" : "", marks
        job = ""; marks = ""; job_token = 0; in_steps = 0
      }
      # A workflow-level `env:` is in scope for every job in the file.
      /^jobs:/ { in_jobs = 1; next }
      !in_jobs {
        if ($0 ~ /NODE_AUTH_TOKEN[[:space:]]*[:=]/) file_token = 1
        next
      }
      /^[^ ]/  { flush_job(); in_jobs = 0; next }
      # A comment naming a variable is prose, the way one naming a command is.
      /^ *#/ { next }
      /^  [A-Za-z0-9_-]+:/ { flush_job(); job = $1; sub(/:$/, "", job); next }
      # Everything before `steps:` is the job unwrapping its own keys, which is
      # where a job-level `env:` lives. `needs:` has `- ` items of its own, so
      # a step is only a step once `steps:` has been seen.
      !in_steps {
        if ($0 ~ /^ *steps:/) { in_steps = 1; next }
        if ($0 ~ /NODE_AUTH_TOKEN[[:space:]]*[:=]/) job_token = 1
        next
      }
      # A four-space key is the job again, not a step: the block has ended.
      /^    [A-Za-z0-9_-]+:/ { flush_step(); in_steps = 0; next }
      /^ *- / { flush_step(); in_step = 1; step = $0; next }
      in_step { step = step " " $0 }
      END { flush_job() }
    ' "$workflow"
  done
}

findings=$(scan)

echo "$findings" | grep '^RUNS ' | while read -r _ workflow job; do
  echo "$workflow: '$job' runs the workspace suite"
done

if ! echo "$findings" | grep -q '^RUNS '; then
  echo "no job runs the workspace suite — this check has stopped checking anything" >&2
  exit 1
fi

if echo "$findings" | grep -q '^MISSING '; then
  echo >&2
  echo "$findings" | grep '^MISSING ' | while read -r _ workflow job name needed_by; do
    echo "$workflow: '$job' runs the workspace suite without $name." >&2
    echo "  $needed_by starts it, and fails rather than skips when it is absent." >&2
  done
  echo >&2
  echo "a job runs the workspace suite without a runtime the suite starts" >&2
  exit 1
fi

# And every job that installs Deno for the suite asks for the same Deno.
# Present was not enough: `publish.yml` installed `v1.x` while `ci.yml`
# installed `v2.x`, and `uf@0.0.0-alpha.36` failed every `deno_host` test in the
# publish job, before a single package was sent, on a Deno too old for
# `registerHooks`.
deno_versions=$(echo "$findings" | grep '^DENO ' | awk '{print $4}' | sort -u)
if [ "$(printf '%s\n' "$deno_versions" | grep -c .)" -gt 1 ]; then
  echo >&2
  echo "$findings" | grep '^DENO ' | while read -r _ workflow job version; do
    echo "$workflow: '$job' installs Deno $version." >&2
  done
  echo >&2
  echo "jobs running the workspace suite install different Deno versions" >&2
  exit 1
fi

# And every job that runs the suite can run the managers it installed. Only the
# jobs the first pass found, so a registry configured in a job that runs no
# suite — `publish.yml`'s `verify`, which installs from npm and tests nothing —
# is not this check's business.
suite_jobs=" $(echo "$findings" | grep '^RUNS ' | awk '{print $2 "/" $3}' | tr '\n' ' ')"

# A function rather than the body of the `$( … )` below, for the same reason
# `scan` is one: bash 3.2 — `/bin/sh` on macOS — cannot parse a `case` inside a
# command substitution, and `scripts:parse` runs `sh -n` with whatever `sh` is.
registry_without_token() {
  scan_npmrc | while read -r _ workflow job marks; do
    case "$suite_jobs" in
      *" $workflow/$job "*) ;;
      *) continue ;;
    esac
    registry_configured=0
    token_in_scope=0
    for mark in $marks; do
      case "$mark" in J) token_in_scope=1 ;; esac
      case "$mark" in
        *S*)
          if [ "$registry_configured" -eq 1 ] && [ "$token_in_scope" -eq 0 ]; then
            case "$mark" in
              *T*) ;;
              *) echo "$workflow $job" ;;
            esac
          fi
          ;;
      esac
      case "$mark" in *R*) registry_configured=1 ;; esac
    done
  done
}

unauthenticated=$(registry_without_token)

if [ -n "$unauthenticated" ]; then
  echo >&2
  echo "$unauthenticated" | while read -r workflow job; do
    echo "$workflow: '$job' runs the workspace suite under a publishing registry with no NODE_AUTH_TOKEN." >&2
    echo "  setup-node's registry-url: writes an .npmrc holding _authToken=\${NODE_AUTH_TOKEN}, and Yarn 1" >&2
    echo "  expands it on every command: crates/uf_cli/tests/managers.rs fails every Yarn 1 row while the" >&2
    echo "  variable is unset, which is how uf@0.0.0-alpha.38 published nothing." >&2
    echo "  Set NODE_AUTH_TOKEN for the step that runs the suite, or configure the registry after it." >&2
  done
  echo >&2
  echo "a job runs the workspace suite under a registry-authenticated .npmrc with no NODE_AUTH_TOKEN" >&2
  exit 1
fi

echo "every job running the workspace suite installs the runtimes it starts, and can run them"
