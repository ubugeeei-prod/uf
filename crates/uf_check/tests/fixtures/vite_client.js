// @flow
//
// The three Vite client APIs a real application uses. Before uf shipped a
// libdef for them this file was five type errors; see ubugeeei-prod/uf#264.

type PageModule = { default: () => string };

// A glob with no options is lazy: each match is a thunk.
const pages = import.meta.glob<PageModule>("./pages/*.js");
const load: () => Promise<PageModule> = pages["./pages/home.js"];

// With `eager`, each match is the module itself.
const eager = import.meta.glob<PageModule>("./pages/*.js", { eager: true });
const home: PageModule = eager["./pages/home.js"];

// The rest of the options, so a change to one of them fails here.
const raw = import.meta.glob("./pages/*.md", {
  query: "?raw",
  import: "default",
  base: "./src",
  caseSensitive: true,
});

// A glob with no type argument still types. Its exports are `unknown`, which
// is the truth about a module nobody has described, so reading one needs a
// check rather than a cast.
const described: { [key: string]: () => Promise<ModuleNamespace> } = raw;

// `import.meta.hot` is undefined in a build, so it is reached through `?.`.
// `if (import.meta.hot)` cannot work: Flow keys refinements by a lookup rooted
// at an identifier, and `import.meta` is a meta-property.
import.meta.hot?.accept();
import.meta.hot?.accept((module) => console.log(module));
import.meta.hot?.accept("./sibling.js", (module) => module);
import.meta.hot?.accept(["./a.js", "./b.js"], (modules) => modules.length);
import.meta.hot?.dispose((data) => {
  data.socket = null;
});
import.meta.hot?.prune((data) => data);
import.meta.hot?.invalidate("the store changed shape");
import.meta.hot?.on("uf:reload", (payload) => payload);
import.meta.hot?.send("uf:ready", { at: Date.now() });
const carried: unknown = import.meta.hot?.data.socket;

// The env is substituted at build time. The five Vite names are typed; a key
// out of a `.env` file is a string that may not be set.
const mode: string = import.meta.env.MODE;
const base: string = import.meta.env.BASE_URL;
const prod: boolean = import.meta.env.PROD;
const dev: boolean = import.meta.env.DEV;
const ssr: boolean = import.meta.env.SSR;
const api: string = import.meta.env.VITE_API_URL ?? "http://localhost:3000";

// What Flow's own core.js already promised is still promised.
const url: string = new URL("./logo.png", import.meta.url).href;

export default {
  load,
  home,
  described,
  carried,
  mode,
  base,
  prod,
  dev,
  ssr,
  api,
  url,
};
