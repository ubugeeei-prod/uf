// @flow
//
// The terminal clipboard this package implements: OSC 52, write-only.
//
// OSC 52 is not a clipboard *service* in the browser sense. It is an escape
// sequence sent to the terminal emulator, and the emulator may accept it,
// ignore it, ask the reader, or cap the payload. There is no acknowledgement
// on the same stream, so `copy()` reports whether uf could ask, not whether a
// terminal eventually changed its clipboard.

/** What `useClipboard()` hands back. */
export type Clipboard = {
  /** Put text on the terminal clipboard. */
  copy(text: string): boolean,
  /** Whether this renderer has a clipboard transport at all. */
  supported: boolean,
};

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/** A clipboard for renderers that have nowhere to send OSC 52. */
export const unsupportedClipboard: Clipboard = {
  supported: false,
  copy() {
    return false;
  },
};

/** A clipboard that writes OSC 52 to a terminal stream. */
export function osc52Clipboard(write: (chunk: string) => mixed): Clipboard {
  return {
    supported: true,
    copy(text: string): boolean {
      write(osc52(text));
      return true;
    },
  };
}

/** Encode text as the OSC 52 "copy to clipboard" sequence. */
export function osc52(text: string): string {
  return `\u001b]52;c;${base64Utf8(text)}\u0007`;
}

function base64Utf8(text: string): string {
  return base64Bytes(new TextEncoder().encode(text));
}

function base64Bytes(bytes: Uint8Array): string {
  let out = "";
  for (let at = 0; at < bytes.length; at += 3) {
    const first = bytes[at];
    const second = bytes[at + 1];
    const third = bytes[at + 2];
    out += ALPHABET[first >> 2];
    out += ALPHABET[((first & 0b00000011) << 4) | ((second ?? 0) >> 4)];
    out += second == null ? "=" : ALPHABET[((second & 0b00001111) << 2) | ((third ?? 0) >> 6)];
    out += third == null ? "=" : ALPHABET[third & 0b00111111];
  }
  return out;
}
