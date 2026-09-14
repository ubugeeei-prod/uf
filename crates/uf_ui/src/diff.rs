//! How a project's copy differs from the registry's component.

use similar::TextDiff;

/// A unified diff from the registry's text to the copy's, or `None` when the
/// two are the same.
///
/// The registry is the old side and the copy the new one, so a `+` line is
/// something the copy has that the registry does not: read that way, the diff
/// of a copy somebody edited is their edit.
pub fn unified(
    registry_label: &str,
    copy_label: &str,
    registry: &str,
    copy: &str,
) -> Option<String> {
    if registry == copy {
        return None;
    }
    let diff = TextDiff::from_lines(registry, copy);
    Some(
        diff.unified_diff()
            .context_radius(3)
            .header(registry_label, copy_label)
            .to_string(),
    )
}
