// @noflow
// Deno 2.9.x routes native addons through a registered load hook as
// JavaScript, even when the hook delegates to the runtime. Load the native
// binding before the Flow hook so the Vite library tests can exercise it.
// This runs only in the CI library lane, not in applications.
import "rolldown";
