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
// hang a comment on. Its body is an object type: `,` between members, and one
// line when it fits. The `;` and the line per member belong to the
// declaration forms below it.
type Anonymous = interface extends Base { m(): boolean };
type Members = interface { a: string, b: number };
type Empty = interface {};
type Wide = interface extends Base { aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa: string, bbbbbbbbbbbbbbbbbbbbbb: number };

declare interface Declared {
  m(): boolean;
}

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
