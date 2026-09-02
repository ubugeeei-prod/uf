// @flow
//
// `@uniflowed/cli`.

import type { NativeHandle } from "@uniflowed/core/native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/cli";

export opaque type CliApp = NativeHandle<"@uniflowed/core/cli#CliApp">;
export opaque type CliCommand = NativeHandle<"@uniflowed/core/cli#CliCommand">;
export opaque type CliArgument = NativeHandle<"@uniflowed/core/cli#CliArgument">;
export opaque type CliOption = NativeHandle<"@uniflowed/core/cli#CliOption">;

export function defineCli(name: string): CliApp {
  return nativeRuntimeRequired(MODULE, "defineCli");
}

export function command(name: string): CliCommand {
  return nativeRuntimeRequired(MODULE, "command");
}

export function arg(name: string): CliArgument {
  return nativeRuntimeRequired(MODULE, "arg");
}

export function option(name: string): CliOption {
  return nativeRuntimeRequired(MODULE, "option");
}
