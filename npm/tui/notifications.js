"use flow";
//
// A desktop notification this package can ask a terminal for.
//
// Two sequences, and no third. OSC 9 is one string, which iTerm2, Ghostty,
// kitty and others show as a notification. OSC 777 is a title and a body,
// which WezTerm and VTE show. OSC 99 is the one with identifiers, icons and
// buttons, and it is not written here: it is a conversation, and this is one
// escape sequence.
//
// Nothing in this file asks the terminal which of those it implements. A
// query would be a probe, and a probe that times out on a pipe is how a
// renderer blocks a reader who was never going to see a banner. The sequence
// is the one the caller named. An interactive terminal that was not asked
// sends OSC 9, which a terminal that does not know it ignores. `notify()`
// reports whether the sequence was written, not whether a desktop showed it.
// There is no acknowledgement on the stream.

/** Which sequence `notify` writes, or none when this renderer will not. */
export type NotificationProtocol = "osc9" | "osc777" | "none";

/** What `useNotifications()` hands back. */
export type Notifications = {
  /** Ask the terminal to show a notification. `body` may be empty. */
  notify(title: string, body?: string): boolean,
  /** Whether this renderer has a transport at all. */
  supported: boolean,
  /** The sequence that transport writes. `"none"` when it writes nothing. */
  protocol: NotificationProtocol,
};

/** A notifier for a renderer that has nowhere to send a sequence. */
export const unsupportedNotifications: Notifications = {
  supported: false,
  protocol : "none",
  notify() {
    return false;
  },
};

/**
 * The protocol a caller asked for, or nothing when they did not name one.
 *
 * `OPENTUI_NOTIFICATION_PROTOCOL` is read only as a name: `osc9`, `osc777`,
 * or an off switch. `true` is not a request to go and find out. An
 * unrecognised value is not a protocol, so the caller falls through to OSC 9
 * on an interactive terminal.
 */
export function namedProtocol(value: mixed): "osc9" | "osc777" | "none" | null {
  if (typeof value !== "string") {
    return null;
  }
  const name = value.trim().toLowerCase();
  if (name === "osc9" || name === "osc777") {
    return name;
  }
  if (name === "none" || name === "off" || name === "false" || name === "0") {
    return "none";
  }
  return null;
}

/**
 * Which sequence an interactive terminal gets.
 *
 * The option wins, then the environment variable, then OSC 9. This is not
 * called for a pipe: a pipe has no notifier, whatever the variable says.
 */
export function notificationProtocol(
  requested: "osc9" | "osc777" | false | void,
  fromEnv  : mixed,
): "osc9" | "osc777" | "none" {
  if (requested === false) {
    return "none";
  }
  if (requested === "osc9" || requested === "osc777") {
    return requested;
  }
  return namedProtocol(fromEnv) ?? "osc9";
}

/** A notifier that writes one sequence to a terminal stream. */
export function terminalNotifications(
  protocol: "osc9" | "osc777",
  write   : (chunk: string) => mixed,
): Notifications {
  return {
    supported: true,
    protocol,
    notify(title: string, body?: string): boolean {
      const sequence = notificationSequence(protocol, title, body ?? "");
      if (sequence == null) {
        return false;
      }
      write(sequence);
      return true;
    },
  };
}

/**
 * The escape sequence for one notification, or nothing when the text cannot
 * be put inside one.
 *
 * A notification is plain text sitting inside an OSC. BEL, ESC, and the C1
 * controls end that OSC or start another, so a title that contains one is
 * refused rather than written: the alternative is a notification that also
 * changes the window title. Newlines become spaces. In OSC 777 a semicolon
 * ends the title, so it is written as a colon.
 */
export function notificationSequence(
  protocol: "osc9" | "osc777",
  title   : string,
  body    : string,
): string | null {
  const safeTitle = notificationText(title);
  const safeBody = notificationText(body);
  if (safeTitle == null || safeBody == null) {
    return null;
  }
  if (protocol === "osc9") {
    const message =
      safeTitle === "" ? safeBody : safeBody === "" ? safeTitle : `${safeTitle}: ${safeBody}`;
    if (message === "") {
      return null;
    }
    return `\u001b]9;${message}\u0007`;
  }
  if (safeTitle === "" && safeBody === "") {
    return null;
  }
  return `\u001b]777;notify;${safeTitle.replaceAll(";", ":")};${safeBody.replaceAll(";", ":")}\u0007`;
}

/** Text that can sit inside an OSC, or nothing when it holds a control. */
function notificationText(text: string): string | null {
  let out = "";
  // `new String` because Flow does not let a primitive string stand for an
  // iterable. The wrapper iterates the same code points.
  for (const char of new String(text)) {
    const code = char.codePointAt(0) ?? 0;
    if (code === 9 || code === 10 || code === 13) {
      out += " ";
      continue;
    }
    // C0, DEL, and C1. OSC is ended by BEL and by ST, and ESC starts another.
    if (code < 32 || (code >= 0x7f && code <= 0x9f)) {
      return null;
    }
    out += char;
  }
  return out;
}
