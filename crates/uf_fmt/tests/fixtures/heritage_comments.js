// @flow

// A comment between a name and its `extends` stays where it was written.
// It used to be moved into the type that followed it, which for an
// `eslint-disable-next-line` means it stops suppressing what it was written
// to suppress. See ubugeeei-prod/uf#135.
export interface AssetSymbols // eslint-disable-next-line no-undef
  extends Iterable<[Symbol, {| local: Symbol, loc: ?SourceLocation, meta?: ?Meta |}]> {
  hasExportSymbol(exportSymbol: Symbol): boolean;
}

interface Plain // why
  extends Base {
  m(): boolean;
}

interface WithArgs // why
  extends Base<Element> {
  m(): boolean;
}

// A block comment is not a line comment: it does not end the line, so the
// header stays on one.
interface Inline /* why */ extends Base {
  m(): boolean;
}

// No comment at all: still one line.
interface Quiet extends Base {
  m(): boolean;
}

// More than one target is the other thing that makes the heritage start a
// line. `extends ` keeps its first target and the rest indent under it.
interface Several extends Base, Other {
  m(): boolean;
}

interface SeveralLong
  extends AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA,
    BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB,
    CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC {
  m(): boolean;
}

interface OneLong
  extends AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA {
  m(): boolean;
}

// An `interface { … }` *type* groups on its heritage too, having no name to
// hang a comment on. Its members are not in this fixture: uf separates them
// with `;` where the hermes plugin uses `,`, which is ubugeeei-prod/uf#151
// and nothing to do with heritage.
type Anonymous = interface extends Base {};

declare class Declared // why
  extends Base {
  m(): boolean;
}

declare class DeclaredQuiet extends Base {
  m(): boolean;
}

declare class DeclaredMixed // why
  extends Base mixins Mixin {
  m(): boolean;
}
