export interface ParserOptions {
  strict?: boolean;
}

export declare abstract class ParserBase {
  validate(input: string, options?: ParserOptions): boolean;
}

export declare class RequiredParser extends ParserBase {
  validate(input: string, options: ParserOptions): boolean;
}
