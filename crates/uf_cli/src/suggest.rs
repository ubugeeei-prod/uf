//! "Did you mean …" — finding what someone meant from what they typed.
//!
//! clap does this for subcommand names, and stops there. Everything else uf
//! takes by name it took silently: a mistyped task name got
//! `task "biuld" is not defined in uf.config.js` and no hint that `build` was
//! sitting right there, which is the most common typo anyone makes with this
//! tool because it is the name they type most often.
//!
//! The measure is Damerau–Levenshtein — insertions, deletions, substitutions
//! and *transpositions* — because the typo that matters here is almost always
//! two adjacent letters swapped, and plain Levenshtein charges two edits for
//! that where a human sees one.

#[cfg(test)]
mod tests;

/// How many suggestions to offer at most.
///
/// One is a guess and reads as a correction; a screenful is a list the reader
/// has to search. Three is enough to cover a genuine ambiguity.
const MAX_SUGGESTIONS: usize = 3;

/// The largest edit distance still worth offering, by length of what was typed.
///
/// A fixed threshold is wrong at both ends: at distance 2, `ci` and `up` are
/// "similar" to almost anything short, while a 20-character name with two typos
/// in it is obviously the one that was meant. So the budget grows with length,
/// and a very short word gets almost none.
fn distance_budget(typed: &str) -> usize {
    match typed.chars().count() {
        0..=2 => 0,
        3..=5 => 1,
        6..=10 => 2,
        _ => 3,
    }
}

/// The candidates closest to `typed`, best first.
///
/// A candidate that contains what was typed — or is contained by it — is always
/// offered, whatever the edit distance says: someone who types `test` at a
/// project with `test:lib` and `test:e2e` has not made a typo, they have been
/// insufficiently specific, and both answers are useful. Otherwise a candidate
/// has to be within [`distance_budget`].
///
/// Ties are broken by the candidate's own name so the same input always
/// produces the same list.
pub(crate) fn closest<'a>(
    typed: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    let budget = distance_budget(typed);
    let lowered = typed.to_lowercase();

    let mut scored = candidates
        .into_iter()
        .filter_map(|candidate| {
            let folded = candidate.to_lowercase();
            // Rank 0 is "one of these is a longer or shorter spelling of what
            // you typed", which beats any edit distance. Within it, the
            // candidate closest in length is closest to what was meant, so an
            // exact match comes first and `test` offers `test:lib` before
            // `install:test`.
            if folded.contains(&lowered) || lowered.contains(&folded) {
                let spread = folded.chars().count().abs_diff(lowered.chars().count());
                return Some((0usize, spread, candidate));
            }
            let distance = damerau_levenshtein(&lowered, &folded);
            (distance <= budget).then_some((1, distance, candidate))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(b.2)));
    scored.truncate(MAX_SUGGESTIONS);
    scored.into_iter().map(|(_, _, name)| name).collect()
}

/// Damerau–Levenshtein distance between two strings, over characters.
///
/// The restricted ("optimal string alignment") variant: a substring may be
/// edited once, so `ca` -> `abc` is three edits rather than two. That is the
/// variant every "did you mean" uses, it is the one with a simple bound, and
/// the difference only shows up on inputs far past any threshold worth
/// offering.
///
/// Two rows rather than a full matrix would be the usual optimisation; three
/// are needed to see a transposition, and the names being compared are a
/// handful of characters long, so the table is small either way.
fn damerau_levenshtein(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();

    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let width = right.len() + 1;
    let mut previous_previous = vec![0usize; width];
    let mut previous = (0..width).collect::<Vec<_>>();
    let mut current = vec![0usize; width];

    for (row, &left_char) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, &right_char) in right.iter().enumerate() {
            let substitution = usize::from(left_char != right_char);
            current[column + 1] = (current[column] + 1)
                .min(previous[column + 1] + 1)
                .min(previous[column] + substitution);

            // A transposition: this pair of characters is the previous pair,
            // swapped. One edit, not two.
            if row > 0
                && column > 0
                && left_char == right[column - 1]
                && left[row - 1] == right_char
            {
                current[column + 1] = current[column + 1].min(previous_previous[column - 1] + 1);
            }
        }
        std::mem::swap(&mut previous_previous, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.len()]
}
