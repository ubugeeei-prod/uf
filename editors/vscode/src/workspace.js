// @flow
//
// The settings a uf project wants VS Code to have, and which of them to write.
//
// # Why the extension configures Flow language mode
//
// Turning off JavaScript validation/suggestions leaves TypeScript's hover,
// definitions and semantic coloring active. The `flow` language id is served
// by uf and its grammar; JavaScript/TypeScript providers do not match it.
// The public setTextDocumentLanguage API selects Flow only inside a uf
// project, including multi-root windows. Window-wide file associations are
// left unchanged.
//
// Legacy and unified validation settings remain for projects that explicitly
// keep the JavaScript language id. A value already set is preserved.
//
// # What is written, and when
//
// Only in a folder that is a uf project — one with `uf.config.js` at its root,
// the same test that decides whether a server starts there — and only to that
// folder's `.vscode/settings.json`, so a TypeScript project in the next window,
// or `.ts` files in this one, are untouched (`typescript.validate.enable` is a
// different setting and is never written).
//
// A value the user already set at the workspace or folder level is never
// changed, whichever way it was set: a project that deliberately wants
// TypeScript's diagnostics on its JavaScript keeps them. And the automatic
// half writes once per folder: if someone deletes the line afterwards, that
// was a decision, and the extension does not put it back on the next start.
//
// The rest of the set — uf as the default formatter for JavaScript and JSX —
// is written only when asked, by the "uf: Configure Workspace for Flow"
// command, because choosing a formatter is a project's decision to commit, not
// something an extension should do on install.

"use strict";

// The id this extension is installed as: `publisher.name` from package.json.
const EXTENSION_ID = "uniflowed.uf";

/*::
export type Setting = {
  // The key as it appears in settings.json, without a language scope.
  readonly key: string,
  // `javascript` for `"[javascript]": { … }`, or null for a top-level key.
  readonly language: string | null,
  readonly value: mixed,
  // One line, for the output channel and the confirmation.
  readonly why: string,
};

// `WorkspaceConfiguration.inspect(key)`, cut down to what the plan reads.
export type Inspection = {
  readonly globalValue?: mixed,
  readonly workspaceValue?: mixed,
  readonly workspaceFolderValue?: mixed,
  readonly globalLanguageValue?: mixed,
  readonly workspaceLanguageValue?: mixed,
  readonly workspaceFolderLanguageValue?: mixed,
  readonly defaultValue?: mixed,
  ...
};

export type Step =
  | { readonly kind: "write", readonly setting: Setting }
  | { readonly kind: "unsupported", readonly setting: Setting }
  | { readonly kind: "already", readonly setting: Setting }
  | { readonly kind: "kept", readonly setting: Setting, readonly current: mixed };
*/

const VALIDATION /*: Setting */ = {
  key: "javascript.validate.enable",
  language: null,
  value: false,
  why: "VS Code's built-in TypeScript service reads Flow syntax as TypeScript errors; uf reports this folder's problems instead",
};

const VALIDATION_UNIFIED /*: Setting */ = {
  key: "js/ts.validate.enabled",
  language: "javascript",
  value: false,
  why: "the same switch under the name VS Code 1.110 and later read first; [javascript] covers .jsx as well",
};

const FORMATTER_FLOW /*: Setting */ = {
  key: "editor.defaultFormatter",
  language: "flow",
  value: EXTENSION_ID,
  why: "format Flow documents with this project's uf",
};

const FORMATTER_JS /*: Setting */ = {
  key: "editor.defaultFormatter",
  language: "javascript",
  value: EXTENSION_ID,
  why: "Format Document and format on save use uf fmt, with this project's uf.config.js",
};

const FORMATTER_JSX /*: Setting */ = {
  key: "editor.defaultFormatter",
  language: "javascriptreact",
  value: EXTENSION_ID,
  why: "the same, for .jsx files",
};

// What the extension may write on its own, in a uf project.
const AUTOMATIC /*: $ReadOnlyArray<Setting> */ = [VALIDATION, VALIDATION_UNIFIED];

// What "uf: Configure Workspace for Flow" writes.
const RECOMMENDED /*: $ReadOnlyArray<Setting> */ = [
  VALIDATION,
  VALIDATION_UNIFIED,
  FORMATTER_FLOW,
  FORMATTER_JS,
  FORMATTER_JSX,
];

/**
 * The value this project set for `setting`, if it set one.
 *
 * Workspace and folder levels only: the user's own settings are theirs for
 * every project, and a project that wants uf's behaviour says so in its own
 * files.
 */
function projectValue(setting /*: Setting */, inspection /*: Inspection */) /*: mixed */ {
  if (setting.language != null) {
    if (inspection.workspaceFolderLanguageValue !== undefined) {
      return inspection.workspaceFolderLanguageValue;
    }
    return inspection.workspaceLanguageValue;
  }
  if (inspection.workspaceFolderValue !== undefined) {
    return inspection.workspaceFolderValue;
  }
  return inspection.workspaceValue;
}

/**
 * What to do about one setting, given what is set now.
 */
function planSetting(setting /*: Setting */, inspection /*: Inspection */) /*: Step */ {
  // Every registered setting has a default; one without is a key this VS Code
  // does not know, and writing it would be refused.
  if (inspection.defaultValue === undefined) {
    return { kind: "unsupported", setting };
  }
  const current = projectValue(setting, inspection);
  if (current === undefined) {
    return { kind: "write", setting };
  }
  if (current === setting.value) {
    return { kind: "already", setting };
  }
  return { kind: "kept", setting, current };
}

/**
 * The plan for a list of settings, in order.
 */
function plan(
  settings /*: $ReadOnlyArray<Setting> */,
  inspect /*: (setting: Setting) => Inspection */,
) /*: Array<Step> */ {
  return settings.map((setting) => planSetting(setting, inspect(setting)));
}

/**
 * How a setting is spelled in settings.json, for messages.
 */
function describeSetting(setting /*: Setting */) /*: string */ {
  const value = JSON.stringify(setting.value);
  if (setting.language != null) {
    return `"[${setting.language}]": { "${setting.key}": ${String(value)} }`;
  }
  return `"${setting.key}": ${String(value)}`;
}

/**
 * The lines for the output channel, one per step.
 */
function describePlan(steps /*: $ReadOnlyArray<Step> */) /*: Array<string> */ {
  return steps.map((step) => {
    const spelled = describeSetting(step.setting);
    switch (step.kind) {
      case "write":
        return `  + ${spelled}  — ${step.setting.why}`;
      case "already":
        return `  = ${spelled}  (already set)`;
      case "unsupported":
        return `  - ${spelled}  not written: this editor does not have that setting`;
      default:
        return `  ! ${spelled}  not written: this project sets ${JSON.stringify(step.current) ?? "it"}, which is kept`;
    }
  });
}

module.exports = {
  AUTOMATIC,
  EXTENSION_ID,
  RECOMMENDED,
  describePlan,
  describeSetting,
  plan,
  planSetting,
};
