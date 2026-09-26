// @flow
// Jest's module environment executes RN's own mocks. uf still owns discovery,
// scheduling, hooks, timeouts, isolation and the result protocol.

import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import * as tests from "../index.js";

// Optional, application-owned environment modules cross one dynamic boundary.
type EnvironmentModule = $FlowFixMe;

export async function loadNativeFile(file: string): Promise<() => Promise<void>> {
  const rootDir = process.cwd();
  const require = createRequire(path.join(rootDir, "package.json"));
  const load = (name: string): EnvironmentModule => {
    try {
      return require(name);
    } catch (cause) {
      throw new Error(
        `uf test (react-native): install ${name} in the application; the native environment could not load it`,
        { cause },
      );
    }
  };
  const Runtime = load("jest-runtime").default;
  const { readConfig } = load("jest-config");
  const { createScriptTransformer } = load("@jest/transform");
  const { jestExpect } = load("@jest/expect");
  const { projectConfig: config, globalConfig } = await readConfig(
    {
      config: JSON.stringify({
        rootDir,
        preset: "@react-native/jest-preset",
        cacheDirectory: path.join(rootDir, ".uf", "native-test-cache"),
        moduleNameMapper: {
          "^@uniflowed/test$": fileURLToPath(new URL("./native-globals.js", import.meta.url).href),
        },
        transform: {
          "^.+\\.[jt]sx?$": require.resolve("@uniflowed/react-native/test-transformer.cjs"),
        },
        transformIgnorePatterns: [
          "node_modules/(?!((jest-)?react-native|@react-native(-community)?|@react-navigation|@uniflowed|react-native-screens|react-native-safe-area-context)/)",
        ],
      }),
    },
    rootDir,
  );
  const context = await Runtime.createContext(config, {
    console,
    maxWorkers: 1,
    watch: false,
    watchman: false,
  });
  const Environment = load(config.testEnvironment);
  const environment = new (Environment.TestEnvironment ?? Environment)(
    { projectConfig: config, globalConfig },
    { console, testPath: file, docblockPragmas: {} },
  );
  await environment.setup();
  let runtime: EnvironmentModule = null;
  let closed = false;
  const close = async () => {
    if (closed) return;
    closed = true;
    try {
      runtime?.leaveTestCode();
      runtime?.teardown();
    } finally {
      await environment.teardown();
    }
  };
  try {
    const transformer = await createScriptTransformer(config, new Map());
    runtime = new Runtime(
      config,
      environment,
      context.resolver,
      transformer,
      new Map(),
      { collectCoverage: false, collectCoverageFrom: [], coverageProvider: "v8" },
      file,
      globalConfig,
    );
    const api = { ...tests, expect: jestExpect };
    for (const [key, value] of Object.entries(api)) environment.global[key] = value;
    environment.global.__UF_NATIVE_TEST_API__ = api;
    runtime.setGlobalsForRuntime(api);
    runtime.enterTestCode();
    for (const setup of config.setupFiles) runtime.requireModule(setup);
    runtime.requireModule(file);
    return close;
  } catch (error) {
    await close();
    throw error;
  }
}
