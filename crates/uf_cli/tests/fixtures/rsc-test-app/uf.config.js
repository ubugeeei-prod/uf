// @flow
import { defineConfig } from "@uniflowed/config";
export default defineConfig({
  app: {
    router: { root: "app", entry: "app.js" },
    // `listNotes` is a cached function; `saveNote` invalidates it by tag.
    rendering: { cache: { data: true } },
  },
});
