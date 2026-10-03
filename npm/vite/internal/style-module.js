// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.

/**
 * The CSS a StyleX virtual module compiled, ignoring Vite's `?direct` query.
 *
 * A `<link rel="stylesheet">` is requested with `Accept: text/css`, and Vite
 * rewrites that request to `?direct`. The map is keyed by the id the
 * transform imported. Including the query looks up nothing, the link is an
 * empty sheet, and a server component's rules never reach a page whose
 * client bundle does not import that module.
 *
 * @param {Map<string, string>} styles
 * @param {string} id
 */
export function styleModuleSource(styles, id) {
  const query = id.indexOf("?");
  return styles.get(query === -1 ? id : id.slice(0, query)) ?? "";
}
