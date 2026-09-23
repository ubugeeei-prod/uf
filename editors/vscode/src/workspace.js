// @flow
//
// The settings a uf project wants VS Code to have, and which of them to write.
//
// # Why the extension writes any settings at all
//
// VS Code ships TypeScript's language service for JavaScript, and it reads a
// Flow file as JavaScript: `component Button()`, `hook useThing()`, `match`,
// `renders` and every type annotation come back as TypeScript errors
// ("Type annotations can only be used in TypeScript files"), under uf's own
// findings for the same lines. Flow's own VS Code extension has asked its
// users to turn that off by hand since it was written.
//
// The switch is the validation setting, and it has two names. VS Code 1.110
// (February 2026) moved it to `js/ts.validate.enabled`, language-overridable,
// and still reads the old `javascript.validate.enable` — but only while the
// new key has no value anywhere, a `"[typescript]"` block in someone's user
// settings included. So both are written: the old name for older VS Code and
// for Cursor (whose extension API is older than 1.110), and the new one
// scoped to `"[javascript]"`, which is the language id the built-in service
// reads it under for `.jsx` files too. Either turns off syntax, semantic and
// suggestion diagnostics for JavaScript and nothing else: hover, go to
// definition and suggestions from the built-in service stay, beside uf's,
// because VS Code has no setting that turns those off short of disabling the
// built-in extension (see the editors guide).
//
// A VS Code that does not know the new key is not written to under it: the
// plan says "unsupported", because VS Code refuses to write a setting nothing
// registered.
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
