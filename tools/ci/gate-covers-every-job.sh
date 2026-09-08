#!/bin/sh
# The `CI` gate names every job in `ci.yml`, and no required context can skip.
#
# `main` was left not compiling on 2026-09-06 with a green tick on it. Every
# job in `ci.yml` declares `needs: toolchain`, because `Toolchain` builds the
# `uf` binary the rest of the pipeline runs its checks through. A job whose
# dependency fails is not run, and a job that is not run reports `skipped` —
# and GitHub counts a skipped required check as satisfied. So a `Toolchain`
# that failed to compile turned `Format`, `Clippy`, `Test`, `Bench Compile` and
# `Metadata` into passes, and an armed auto-merge fired. See #367.
#
# The `CI` job is the answer to that: it runs whatever happened above it
# (`if: always()`) and is red unless every job it names came back `success`,
# so a skip is a failure there. But it is only an answer while it names every
# job — and its list of names is a list somebody has to add to. That is the
# same shape as the bug it fixes: `Flow lint` became a job in #404 and was in
# nobody's required set, and a merge was free to ignore it for a day. A gate
# that silently stops covering a job is a gate that reports `success` for a
# check nothing ran, which is exactly where this started.
#
# So the list is checked rather than trusted. This reads `.github/workflows/`
# and asserts four things:
#
#   1. The gate exists, is named `CI`, and is `if: always()` — without that it
#      skips along with everything else and reports nothing at all.
#   2. Every other job in `ci.yml` is either in the gate's `needs:` or is
#      excluded on purpose, in writing, by a marker comment that names it.
#   3. Nothing in the gate's `needs:` is a job that no longer exists. A `needs:`
#      naming a deleted job is not a soft failure — GitHub refuses to run the
#      workflow at all, and the required contexts then never report.
#   4. Each context branch protection requires is a job that cannot skip:
#      inside `ci.yml` that means the gate covers it, and outside it means the
#      job has no `needs:` of its own. Either way it must carry no job-level
#      `if:`, which is the other way a job reports `skipped`.
#   5. Every workflow that reports a required context runs on `merge_group`.
#      The merge queue merges a batch when the required checks pass on its
#      speculative merge commit; a required context whose workflow has no
#      `merge_group` trigger never reports there, and the queue waits on a
#      check that is never coming. See #588.
#
# What this cannot check is branch protection itself — whether `CI` is in the
# required set for `main` is a repository setting, readable only with admin
# rights, and a check that could widen the set of checks it must pass would not
# be a check. The list below is this repository's written record of what that
# setting is meant to say; #367 is where it is argued.
#
# The parser is line-oriented rather than a YAML library, because this runs in
# the `Metadata` job, which installs nothing — no `npm ci`, no `pip`. It knows
# the shape these files are written in (two spaces to a job key, four to its
# keys, `needs:` as a block sequence) and fails loudly on a file it cannot
# read, which is the safe direction for a gate.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

workflows=".github/workflows"
ci_workflow="$workflows/ci.yml"

# The job id of the gate, and the check-run name it reports under. The name is
# the thing branch protection names, so the two have to be kept together.
gate_job="ci"
gate_context="CI"

# The contexts required on `main`, newline-separated because two of them have a
# space in them.
#
# All seven are what branch protection lists today. `CI` was the last to be
# added: #367 asked for it, the gate spent a day advisory — red while nothing
# stopped the merge, which is what happened on 2026-09-07 while `main` was
# failing it — and the setting has since caught up.
#
# The list stays here rather than being read from the API. Everything asserted
# below is true whether or not the setting agrees, and this file is then the
# one place in the repository that says what the setting is supposed to be.
required_contexts="Format
Clippy
Test
Bench Compile
Metadata
Zizmor
CI"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-gate-covers.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

errors=0
fail() {
  printf '  FAIL  %s\n' "$1" >&2
  errors=$((errors + 1))
}

# One workflow file to tab-separated records, one per line:
#
#   job      <id>
#   name     <id>  <display name>
#   if       <id>  <expression>
#   need     <id>  <dependency>
#   exclude  <id>  <the rest of the marker comment>
#
# The exclusion marker is read from anywhere in the file and names its own job,
# so it cannot drift onto a neighbour the way a comment attached by position
# can: `# ci-gate excludes upstream-flow — <why>`.
parse() {
  awk '
    BEGIN { injobs = 0; job = ""; needs_block = 0 }
    {
      line = $0

      # Comments first: the marker is a comment, and no other rule below
      # should see one.
      if (line ~ /^[[:space:]]*#/) {
        if (match(line, /ci-gate excludes [A-Za-z0-9_-]+/)) {
          rest = substr(line, RSTART + 17)
          id = rest
          sub(/[^A-Za-z0-9_-].*$/, "", id)
          reason = substr(rest, length(id) + 1)
          # The marker reads as a sentence, so it is usually written inside
          # backticks. Those are punctuation around it, not part of the reason.
          sub(/^[[:space:]`]*/, "", reason)
          sub(/[[:space:]]*$/, "", reason)
          printf "exclude\t%s\t%s\n", id, reason
        }
        next
      }

      # A key at column 0 opens the jobs region or closes it. `on:` has keys
      # two spaces in as well, so the region matters.
      if (line ~ /^[^[:space:]]/) {
        injobs = (line ~ /^jobs:[[:space:]]*$/)
        job = ""
        needs_block = 0
        next
      }
      if (!injobs) next
      if (line ~ /^[[:space:]]*$/) next

      # Two spaces, a name, a colon, nothing else: a job.
      if (line ~ /^  [A-Za-z0-9_-]+:[[:space:]]*$/) {
        job = line
        sub(/^  /, "", job)
        sub(/:[[:space:]]*$/, "", job)
        printf "job\t%s\n", job
        needs_block = 0
        next
      }
      if (job == "") next

      # Items of a `needs:` block sequence, until something that is not one.
      if (needs_block) {
        if (line ~ /^      -[[:space:]]+[A-Za-z0-9_-]+[[:space:]]*$/) {
          v = line
          sub(/^      -[[:space:]]+/, "", v)
          sub(/[[:space:]]*$/, "", v)
          printf "need\t%s\t%s\n", job, v
          next
        }
        needs_block = 0
      }

      if (line ~ /^    name:[[:space:]]*[^[:space:]]/) {
        v = line
        sub(/^    name:[[:space:]]*/, "", v)
        sub(/[[:space:]]*$/, "", v)
        gsub(/^"|"$/, "", v)
        printf "name\t%s\t%s\n", job, v
        next
      }

      if (line ~ /^    if:[[:space:]]*[^[:space:]]/) {
        v = line
        sub(/^    if:[[:space:]]*/, "", v)
        sub(/[[:space:]]*$/, "", v)
        printf "if\t%s\t%s\n", job, v
        next
      }

      if (line ~ /^    needs:[[:space:]]*$/) { needs_block = 1; next }

      # `needs: [a, b]` and `needs: a`, which nothing here writes today but a
      # reader of this file is entitled to.
      if (line ~ /^    needs:[[:space:]]*\[/) {
        v = line
        sub(/^    needs:[[:space:]]*\[/, "", v)
        sub(/\].*$/, "", v)
        n = split(v, parts, /,/)
        for (i = 1; i <= n; i++) {
          p = parts[i]
          gsub(/[^A-Za-z0-9_-]/, "", p)
          if (p != "") printf "need\t%s\t%s\n", job, p
        }
        next
      }
      if (line ~ /^    needs:[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*$/) {
        v = line
        sub(/^    needs:[[:space:]]*/, "", v)
        sub(/[[:space:]]*$/, "", v)
        printf "need\t%s\t%s\n", job, v
        next
      }
    }
  ' "$1"
}

# Does $1 declare `merge_group` in its `on:` block?
#
# Line-oriented for the same reason `parse` is: this runs in the `Metadata`
# job, which installs nothing. `on:` is a key at column 0 and closes at the
# next one, so a `merge_group` inside that region counts however it is
# written — bare, or with keys under it.
declares_merge_group() {
  awk '
    /^[^[:space:]#]/ { inon = ($0 ~ /^on:[[:space:]]*$/); next }
    inon && $0 ~ /^[[:space:]]+merge_group:/ { found = 1 }
    END { exit found ? 0 : 1 }
  ' "$1"
}

# Is $1 one of the space-separated words in $2? Job ids have no spaces in them.
contains() {
  case " $2 " in
    *" $1 "*) return 0 ;;
  esac
  return 1
}

test -f "$ci_workflow" || {
  printf 'gate-covers-every-job: no %s\n' "$ci_workflow" >&2
  exit 1
}

parse "$ci_workflow" > "$work/ci.records"

jobs="$(awk -F'\t' '$1 == "job" { printf "%s ", $2 }' "$work/ci.records")"
if [ -z "$jobs" ]; then
  printf 'gate-covers-every-job: read no jobs at all out of %s.\n' "$ci_workflow" >&2
  printf 'That is this script failing to parse the file, not the file being empty.\n' >&2
  exit 1
fi

echo "gate-covers-every-job: $ci_workflow"

# 1. The gate itself.
if ! contains "$gate_job" "$jobs"; then
  fail "$ci_workflow has no \`$gate_job\` job — the gate is the whole fix for #367"
  printf '\nWithout it a failing `Toolchain` skips every other job, and GitHub reads a\n' >&2
  printf 'skipped required check as a pass.\n' >&2
  exit 1
fi

gate_name="$(awk -F'\t' -v j="$gate_job" '$1 == "name" && $2 == j { print $3 }' "$work/ci.records")"
if [ "$gate_name" != "$gate_context" ]; then
  fail "the \`$gate_job\` job reports as '$gate_name', not '$gate_context' — branch protection names the check, so renaming it silently drops the requirement"
fi

gate_if="$(awk -F'\t' -v j="$gate_job" '$1 == "if" && $2 == j { print $3 }' "$work/ci.records")"
if [ "$gate_if" != "always()" ]; then
  fail "the \`$gate_job\` job is \`if: $gate_if\`, not \`if: always()\` — a gate that skips when its dependencies fail is the bug, not the fix"
fi

gate_needs="$(awk -F'\t' -v j="$gate_job" '$1 == "need" && $2 == j { printf "%s ", $3 }' "$work/ci.records")"

# 2. Exclusions, which have to name a real job, say why, and not also be in the
#    list — a job that is both is a contradiction, and the `needs:` wins
#    silently.
excluded=""
while IFS='	' read -r id reason; do
  [ -n "$id" ] || continue
  if ! contains "$id" "$jobs"; then
    fail "\`ci-gate excludes $id\` names a job that is not in $ci_workflow"
    continue
  fi
  if contains "$id" "$gate_needs"; then
    fail "\`$id\` is excluded by a marker comment and is also in the gate's \`needs:\` — say one thing"
    continue
  fi
  # Counted as excluded before the reason is judged, so that a marker with a
  # thin reason is reported once — as a thin reason — rather than twice, as a
  # thin reason and an uncovered job.
  excluded="$excluded$id "
  # A marker with nothing behind it is a waiver nobody had to defend, and a
  # gate empties out one undefended waiver at a time. Eight characters is not
  # a high bar; writing the sentence is the point of it.
  if [ "${#reason}" -lt 8 ]; then
    fail "\`ci-gate excludes $id\` gives no reason — a job kept out of the gate needs the sentence that argues it, on the line"
    continue
  fi
  printf '  excluded   %s: %s\n' "$id" "$reason"
done <<EOF
$(awk -F'\t' '$1 == "exclude" { printf "%s\t%s\n", $2, $3 }' "$work/ci.records")
EOF

# 3. Every job is covered, and everything covered is a job. The gate is not in
#    its own `needs:`, and a job cannot depend on itself.
covered=0
total=0
for job in $jobs; do
  if [ "$job" = "$gate_job" ]; then
    continue
  fi
  total=$((total + 1))
  if contains "$job" "$gate_needs"; then
    covered=$((covered + 1))
    continue
  fi
  if contains "$job" "$excluded"; then
    continue
  fi
  fail "\`$job\` is in $ci_workflow and in neither the \`$gate_job\` job's \`needs:\` nor an exclusion — a job worth running is a job worth failing on. Add it to \`needs:\`, or write \`# ci-gate excludes $job — <why>\` next to it."
done

for need in $gate_needs; do
  contains "$need" "$jobs" \
    || fail "the \`$gate_job\` job needs \`$need\`, which is not a job in $ci_workflow — GitHub refuses to run a workflow with a dangling \`needs:\` at all, and the required contexts then never report"
done

printf '  covered    %s of %s jobs\n' "$covered" "$total"

# 4. The required contexts. Every workflow, because `Zizmor` is not in this one.
: > "$work/table"
for file in "$workflows"/*.yml; do
  parse "$file" > "$work/records"
  awk -F'\t' -v file="$file" '
    $1 == "job"  { jobs[++n] = $2 }
    $1 == "name" { name[$2] = $3 }
    $1 == "need" { needs[$2] = 1 }
    $1 == "if"   { cond[$2] = $3 }
    END {
      for (i = 1; i <= n; i++) {
        j = jobs[i]
        printf "%s\t%s\t%s\t%s\t%s\n", file, j, (j in name) ? name[j] : j,
          (j in needs) ? "needs" : "-", (j in cond) ? cond[j] : "-"
      }
    }
  ' "$work/records" >> "$work/table"
done

# A file rather than a pipeline, so the loop body runs in this shell and what
# it counts survives it. One context per line, because two have a space in
# them.
printf '%s\n' "$required_contexts" > "$work/required"
while IFS= read -r context; do
  [ -n "$context" ] || continue
  match="$(awk -F'\t' -v c="$context" '$3 == c { print; exit }' "$work/table")"
  if [ -z "$match" ]; then
    fail "no job in $workflows/ reports as \`$context\`, and branch protection requires it — a required context nothing reports blocks every pull request until somebody with admin rights notices. See #197, which is the last time a deleted job left one behind."
    continue
  fi
  file="$(printf '%s\n' "$match" | cut -f1)"
  id="$(printf '%s\n' "$match" | cut -f2)"
  has_needs="$(printf '%s\n' "$match" | cut -f4)"
  cond="$(printf '%s\n' "$match" | cut -f5)"

  if [ "$file" = "$ci_workflow" ] && [ "$id" != "$gate_job" ]; then
    contains "$id" "$gate_needs" \
      || fail "\`$context\` is required and the gate does not name it — a required context that can skip is the whole of #367"
  elif [ "$file" != "$ci_workflow" ] && [ "$has_needs" = "needs" ]; then
    fail "\`$context\` is required, lives in $file, and declares \`needs:\` — so it reports \`skipped\` when what it needs fails, and GitHub reads that as a pass. It wants a gate of its own, the way \`ci.yml\` has one."
  fi

  # `always()` is the one condition that cannot make a job skip, which is why
  # the gate carries it; any other condition can, and a skipped required check
  # is read as satisfied.
  if [ "$cond" != "-" ] && [ "$cond" != "always()" ]; then
    fail "\`$context\` is required and carries \`if: $cond\` — a job whose condition is false reports \`skipped\`, which is a pass, and that is the same hole by a different route"
  fi

  printf '%s\n' "$file" >> "$work/context-files"

  printf '  required   %-16s %s (%s)\n' "$context" "$id" "$file"
done < "$work/required"

# 5. And each of those workflows runs on `merge_group`.
#
# One complaint per file rather than one per context, because `ci.yml` reports
# six of the seven and six copies of the same sentence is not six problems.
if [ -f "$work/context-files" ]; then
  sort -u "$work/context-files" | while IFS= read -r file; do
    [ -n "$file" ] || continue
    if declares_merge_group "$file"; then
      printf '  queued     %s\n' "$file"
    else
      printf '  FAIL  %s reports a required context and has no `merge_group` in its `on:` — the merge queue merges a batch when the required checks pass on its speculative merge commit, and a check that never runs there is a check the queue waits on forever. See #588.\n' "$file" >&2
      echo x >> "$work/queue-errors"
    fi
  done
  if [ -f "$work/queue-errors" ]; then
    errors=$((errors + $(wc -l < "$work/queue-errors")))
  fi
fi

if [ "$errors" -ne 0 ]; then
  printf '\n%s problem(s). The gate is what stands between a `Toolchain` that does not\n' "$errors" >&2
  printf 'compile and an auto-merge onto `main`; see #367 for the morning it did not.\n' >&2
  exit 1
fi

echo "gate-covers-every-job: ok"
