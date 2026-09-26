// @noflow
// Deno 2.9.x mishandles native addons and CommonJS JSON dependencies after
// a registered load hook is installed. Load Rolldown's binding and exercise
// Babel's lazy target resolution before the Flow hook takes effect. Relay's
// transform then uses Babel's already loaded CommonJS dependencies.
// This runs only in the CI library lane, not in applications.
import "rolldown";
import "@ox-content/napi";
import { transformAsync } from "@babel/core";
await transformAsync("const x = 1;", { babelrc: false, configFile: false });
