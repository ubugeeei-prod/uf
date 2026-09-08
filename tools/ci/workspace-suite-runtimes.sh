#!/usr/bin/env sh
# Every job that runs the workspace test suite installs the runtimes it starts.
#
# `crates/uf_cli/tests/{bun_host,deno_host,permissions}.rs` start real Bun,
# Deno and Node processes, and they **fail rather than skip** when the runtime
# is absent. That is the policy working — a host claim nobody checked must not
# be green — but it makes `cargo test --workspace` runnable only in a job that
# installed all three.
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

RUNTIMES="bun:oven-sh/setup-bun:crates/uf_cli/tests/bun_host.rs
deno:denoland/setup-deno:crates/uf_cli/tests/deno_host.rs
node:actions/setup-node:crates/uf_cli/tests/permissions.rs"

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
      /run:|uses:/ { body = body " " $0 }
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
    done
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

echo "every job running the workspace suite installs the runtimes it starts"
