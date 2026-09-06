// @flow
//
// Every line here must be an error. A libdef that typed the Vite client as
// `any` would pass the positive fixture beside this one and fail nothing —
// type quality is the feature, so it is asserted directly.

type PageModule = { default: () => string };

// `MODE` is a string, not whatever the reader would like.
const mode: number = import.meta.env.MODE;

// `PROD` is a boolean, and does not become a string on request.
const prod: string = import.meta.env.PROD;

// A key out of a `.env` file may not be set, so it is not a bare string.
const api: string = import.meta.env.VITE_API_URL;

// The lazy form hands back a thunk. Treating it as the module is an error.
const lazy = import.meta.glob<PageModule>("./pages/*.js");
const notAModule: PageModule = lazy["./pages/home.js"];

// The eager form hands back the module. Calling it is an error.
const eager = import.meta.glob<PageModule>("./pages/*.js", { eager: true });
const notAThunk: () => Promise<PageModule> = eager["./pages/home.js"];

// `eager` is a boolean, and the overload that reads it says so.
import.meta.glob("./pages/*.js", { eager: "yes" });

// `hot` is optional, and reaching through it without `?.` is an error. This
// is also the assertion that `if (import.meta.hot)` still does not refine: if
// upstream Flow ever learns to key a refinement on a meta-property, this test
// fails and the note in the libdef needs rewriting.
if (import.meta.hot) {
  import.meta.hot.accept();
}

export default { mode, prod, api, notAModule, notAThunk };
