/**
 * @flow
 * @fileoverview The part of the VS Code extension API `editors/vscode` uses,
 * typed for Flow.
 *
 * `vscode` is not a package: the editor injects it into the extension host,
 * and its types ship as TypeScript (`@types/vscode`). `vscode-languageclient`
 * is a dependency of the extension only, installed when the extension is
 * packaged and not at this repository's root. So at the root `uf check` saw
 * both as untyped, every `vscode.WorkspaceFolder` annotation in the extension
 * was "an any-typed value used as a type", and every call through them went
 * unchecked.
 *
 * This declares what the extension calls, with the signatures
 * `@types/vscode` 1.85 and `vscode-languageclient` 9 give them, and nothing
 * more. A member the extension starts using has to be added here, which is
 * the point: an undeclared one is an error rather than an `any`.
 *
 * It lives in the repository's `flow-typed/`, the directory `uf check` reads
 * library definitions from without configuration.
 */

declare module "vscode" {
  declare export interface Disposable {
    dispose(): mixed;
  }

  declare export type Event<T> = (listener: (event: T) => mixed) => Disposable;

  declare export class Uri {
    readonly fsPath: string;
    readonly scheme: string;
    readonly path: string;
    toString(): string;
  }

  declare export interface WorkspaceFolder {
    readonly uri: Uri;
    readonly name: string;
    readonly index: number;
  }

  declare export class RelativePattern {
    constructor(base: WorkspaceFolder | Uri | string, pattern: string): void;
  }

  declare export class ThemeColor {
    constructor(id: string): void;
  }

  declare export class TextEdit {}

  declare export interface TextDocument {
    readonly uri: Uri;
    readonly languageId: string;
  }

  declare export var languages: {
    match(selector: { readonly pattern: string }, document: TextDocument): number,
    setTextDocumentLanguage(document: TextDocument, languageId: string): Promise<TextDocument>,
  };

  declare export interface TextDocumentWillSaveEvent {
    readonly document: TextDocument;
    waitUntil(thenable: Promise<$ReadOnlyArray<TextEdit>>): void;
  }

  declare export interface WorkspaceFoldersChangeEvent {
    readonly added: $ReadOnlyArray<WorkspaceFolder>;
    readonly removed: $ReadOnlyArray<WorkspaceFolder>;
  }

  declare export interface ConfigurationChangeEvent {
    affectsConfiguration(section: string, scope?: Uri | WorkspaceFolder): boolean;
  }

  /** What `WorkspaceConfiguration.inspect` answers for one key. */
  declare export type ConfigurationInspection = {
    readonly key: string,
    readonly defaultValue?: mixed,
    readonly globalValue?: mixed,
    readonly workspaceValue?: mixed,
    readonly workspaceFolderValue?: mixed,
    readonly defaultLanguageValue?: mixed,
    readonly globalLanguageValue?: mixed,
    readonly workspaceLanguageValue?: mixed,
    readonly workspaceFolderLanguageValue?: mixed,
    readonly languageIds?: $ReadOnlyArray<string>,
  };

  declare export var ConfigurationTarget: {
    readonly Global: 1,
    readonly Workspace: 2,
    readonly WorkspaceFolder: 3,
  };

  declare export type ConfigurationScope =
    | Uri
    | TextDocument
    | WorkspaceFolder
    | { readonly uri?: Uri, readonly languageId: string };

  declare export interface WorkspaceConfiguration {
    get(section: string): mixed;
    inspect(section: string): ConfigurationInspection | void;
    update(
      section: string,
      value: mixed,
      configurationTarget?: 1 | 2 | 3 | boolean | null,
      overrideInLanguage?: boolean,
    ): Promise<void>;
  }

  declare export interface FileSystemWatcher extends Disposable {
    onDidChange: Event<Uri>;
    onDidCreate: Event<Uri>;
    onDidDelete: Event<Uri>;
  }

  declare export interface OutputChannel extends Disposable {
    readonly name: string;
    appendLine(value: string): void;
    show(preserveFocus?: boolean): void;
  }

  declare export var StatusBarAlignment: {
    readonly Left: 1,
    readonly Right: 2,
  };

  declare export interface StatusBarItem extends Disposable {
    name: string | void;
    text: string;
    tooltip: string | void;
    command: string | void;
    backgroundColor: ThemeColor | void;
    show(): void;
    hide(): void;
  }

  declare export interface Memento {
    get(key: string): mixed;
    update(key: string, value: mixed): Promise<void>;
  }

  declare export interface ExtensionContext {
    readonly subscriptions: Array<Disposable>;
    readonly workspaceState: Memento;
  }

  declare export type QuickPickOptions = {
    readonly placeHolder?: string,
  };

  declare export type MessageOptions = {
    readonly modal?: boolean,
    readonly detail?: string,
  };

  declare export var window: {
    createOutputChannel(name: string): OutputChannel,
    createStatusBarItem(id: string, alignment?: 1 | 2, priority?: number): StatusBarItem,
    showQuickPick<T extends { readonly label: string, ... }>(
      items: $ReadOnlyArray<T>,
      options?: QuickPickOptions,
    ): Promise<T | void>,
    // Two overloads, as an intersection: an object type cannot name a
    // property twice.
    showInformationMessage: ((
      message: string,
      ...items: $ReadOnlyArray<string>
    ) => Promise<string | void>) &
      ((
        message: string,
        options: MessageOptions,
        ...items: $ReadOnlyArray<string>
      ) => Promise<string | void>),
    showWarningMessage(message: string, ...items: $ReadOnlyArray<string>): Promise<string | void>,
    showErrorMessage(message: string, ...items: $ReadOnlyArray<string>): Promise<string | void>,
  };

  declare export var workspace: {
    readonly workspaceFolders: $ReadOnlyArray<WorkspaceFolder> | void,
    readonly textDocuments: $ReadOnlyArray<TextDocument>,
    getConfiguration(section?: string, scope?: ConfigurationScope | null): WorkspaceConfiguration,
    getWorkspaceFolder(uri: Uri): WorkspaceFolder | void,
    createFileSystemWatcher(globPattern: RelativePattern | string): FileSystemWatcher,
    onDidChangeWorkspaceFolders: Event<WorkspaceFoldersChangeEvent>,
    onDidChangeConfiguration: Event<ConfigurationChangeEvent>,
    onWillSaveTextDocument: Event<TextDocumentWillSaveEvent>,
    onDidOpenTextDocument: Event<TextDocument>,
  };

  declare export var commands: {
    registerCommand(command: string, callback: (...args: Array<mixed>) => mixed): Disposable,
    executeCommand(command: string, ...rest: $ReadOnlyArray<mixed>): Promise<mixed>,
  };
}

declare module "vscode-languageclient/node" {
  import type { Event, OutputChannel, TextDocument, TextEdit, WorkspaceFolder } from "vscode";

  declare export var State: {
    readonly Stopped: 1,
    readonly Starting: 3,
    readonly Running: 2,
  };

  declare export type StateChangeEvent = {
    readonly oldState: 1 | 2 | 3,
    readonly newState: 1 | 2 | 3,
  };

  /** How to start the server: a command and its arguments. */
  declare export type Executable = {
    readonly command: string,
    readonly args?: $ReadOnlyArray<string>,
    readonly options?: {
      readonly cwd?: string,
      readonly shell?: boolean,
      // `ExecutableOptions` is an interface upstream: more members are allowed.
      ...
    },
  };

  declare export type ServerOptions = { readonly run: Executable, readonly debug: Executable };

  declare export type LanguageClientOptions = {
    readonly documentSelector?: $ReadOnlyArray<{
      readonly scheme?: string,
      readonly pattern?: mixed,
    }>,
    readonly workspaceFolder?: WorkspaceFolder,
    readonly outputChannel?: OutputChannel,
    readonly initializationOptions?: mixed,
  };

  /** The `textDocument/formatting` request, as the client names it. */
  declare export var DocumentFormattingRequest: {
    readonly type: { readonly method: "textDocument/formatting" },
  };

  /** The LSP shapes the extension sends and receives, opaque to it. */
  declare export opaque type ProtocolTextEdit;
  declare export opaque type TextDocumentIdentifier;

  declare export class LanguageClient {
    constructor(
      id: string,
      name: string,
      serverOptions: ServerOptions,
      clientOptions: LanguageClientOptions,
    ): void;
    start(): Promise<void>;
    stop(): Promise<void>;
    onDidChangeState: Event<StateChangeEvent>;
    readonly initializeResult: {
      readonly serverInfo?: { readonly name: string, readonly version?: string },
    } | void;
    readonly code2ProtocolConverter: {
      asTextDocumentIdentifier(document: TextDocument): TextDocumentIdentifier,
    };
    readonly protocol2CodeConverter: {
      asTextEdits(edits: $ReadOnlyArray<ProtocolTextEdit>): Promise<Array<TextEdit>>,
    };
    sendRequest(
      type: { readonly method: "textDocument/formatting" },
      params: {
        readonly textDocument: TextDocumentIdentifier,
        readonly options: { readonly tabSize: number, readonly insertSpaces: boolean },
      },
    ): Promise<$ReadOnlyArray<ProtocolTextEdit> | null>;
  }
}
