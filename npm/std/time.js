// @flow
//
// `@uniflowed/std/time`: durations and cancellable timers.
//
// This is not a clock, formatter or calendar package. JavaScript already has
// `Date`, `Intl.DateTimeFormat`, and Temporal for that. This module is the
// missing utility layer around the one primitive every runtime does share:
// `setTimeout` and `setInterval`, with explicit cancellation so a stopped wait
// releases its pending promises instead of leaving them parked forever.

/** A duration or a millisecond count accepted by timer helpers. */
export type DurationInput = Duration | number;

/** A signed duration measured in milliseconds. */
export class Duration {
  _milliseconds: number;

  constructor(milliseconds: number) {
    this._milliseconds = finite(milliseconds, "duration");
  }

  milliseconds(): number {
    return this._milliseconds;
  }

  seconds(): number {
    return this._milliseconds / 1_000;
  }

  add(other: DurationInput): Duration {
    return new Duration(this._milliseconds + asDuration(other, "duration").milliseconds());
  }

  sub(other: DurationInput): Duration {
    return new Duration(this._milliseconds - asDuration(other, "duration").milliseconds());
  }

  mul(factor: number): Duration {
    return new Duration(this._milliseconds * finite(factor, "factor"));
  }

  div(divisor: number): Duration {
    const value = finite(divisor, "divisor");
    if (value === 0) {
      throw new RangeError("@uniflowed/std/time: divisor must not be zero");
    }
    return new Duration(this._milliseconds / value);
  }

  neg(): Duration {
    return new Duration(-this._milliseconds);
  }

  abs(): Duration {
    return new Duration(Math.abs(this._milliseconds));
  }

  compare(other: DurationInput): -1 | 0 | 1 {
    const right = asDuration(other, "duration").milliseconds();
    if (this._milliseconds < right) {
      return -1;
    }
    if (this._milliseconds > right) {
      return 1;
    }
    return 0;
  }

  toString(): string {
    return `${String(this._milliseconds)}ms`;
  }

  valueOf(): number {
    return this._milliseconds;
  }
}

/** A one-shot timer. `done()` resolves `true` when it fires and `false` when stopped. */
export class Timer {
  _id: TimeoutID | null = null;
  _state: "active" | "fired" | "stopped" = "stopped";
  _callback: (() => mixed) | null = null;
  _waiters: Array<(fired: boolean) => void> = [];

  constructor(delay: DurationInput, callback?: () => mixed) {
    this._callback = callback ?? null;
    this.reset(delay);
  }

  done(): Promise<boolean> {
    switch (this._state) {
      case "fired":
        return Promise.resolve(true);
      case "stopped":
        return Promise.resolve(false);
      default:
        return new Promise((resolve) => {
          this._waiters.push(resolve);
        });
    }
  }

  stop(): boolean {
    if (this._state !== "active") {
      return false;
    }
    if (this._id != null) {
      clearTimeout(this._id);
      this._id = null;
    }
    this._state = "stopped";
    this._settle(false);
    return true;
  }

  reset(delay: DurationInput, callback?: () => mixed): void {
    const nextDelay = timeoutDelay(delay, "timer delay");
    if (callback !== undefined) {
      this._callback = callback;
    }
    if (this._id != null) {
      clearTimeout(this._id);
      this._id = null;
    }
    this._state = "active";
    this._id = setTimeout(() => {
      this._fire();
    }, nextDelay);
  }

  active(): boolean {
    return this._state === "active";
  }

  _fire(): void {
    if (this._state !== "active") {
      return;
    }
    this._id = null;
    this._state = "fired";
    this._settle(true);
    const callback = this._callback;
    if (callback != null) {
      callback();
    }
  }

  _settle(fired: boolean): void {
    const waiters = this._waiters;
    this._waiters = [];
    for (const resolve of waiters) {
      resolve(fired);
    }
  }
}

/** A periodic timer. Slow consumers observe at most one queued tick. */
export class Ticker {
  _id: IntervalID;
  _stopped: boolean = false;
  _pending: number | null = null;
  _waiters: Array<(tick: number | null) => void> = [];

  constructor(interval: DurationInput) {
    const delay = intervalDelay(interval);
    this._id = setInterval(() => {
      this._tick();
    }, delay);
  }

  tick(): Promise<number | null> {
    if (this._pending != null) {
      const tick = this._pending;
      this._pending = null;
      return Promise.resolve(tick);
    }
    if (this._stopped) {
      return Promise.resolve(null);
    }
    return new Promise((resolve) => {
      this._waiters.push(resolve);
    });
  }

  stop(): void {
    if (this._stopped) {
      return;
    }
    clearInterval(this._id);
    this._stopped = true;
    this._pending = null;
    const waiters = this._waiters;
    this._waiters = [];
    for (const resolve of waiters) {
      resolve(null);
    }
  }

  stopped(): boolean {
    return this._stopped;
  }

  _tick(): void {
    if (this._stopped) {
      return;
    }
    const tick = Date.now();
    const resolve = this._waiters.shift();
    if (resolve != null) {
      resolve(tick);
      return;
    }
    this._pending = tick;
  }
}

/** A duration measured in milliseconds. */
export function milliseconds(value: number): Duration {
  return new Duration(value);
}

/** A duration measured in seconds. */
export function seconds(value: number): Duration {
  return new Duration(finite(value, "seconds") * 1_000);
}

/** A duration measured in minutes. */
export function minutes(value: number): Duration {
  return new Duration(finite(value, "minutes") * 60_000);
}

/** A duration measured in hours. */
export function hours(value: number): Duration {
  return new Duration(finite(value, "hours") * 3_600_000);
}

/** Resolve after `delay`. */
export function after(delay: DurationInput): Promise<void> {
  return new Timer(delay).done().then(() => {});
}

/** Run `callback` after `delay`, returning the timer so it can be stopped. */
export function afterFunc(delay: DurationInput, callback: () => mixed): Timer {
  return new Timer(delay, callback);
}

function asDuration(input: DurationInput, name: string): Duration {
  if (input instanceof Duration) {
    return input;
  }
  if (typeof input === "number") {
    return new Duration(input);
  }
  throw new TypeError(`@uniflowed/std/time: ${name} must be a Duration or milliseconds`);
}

function timeoutDelay(input: DurationInput, name: string): number {
  const delay = Math.max(0, Math.ceil(asDuration(input, name).milliseconds()));
  if (delay > MAX_TIMER_DELAY) {
    throw new RangeError(`@uniflowed/std/time: ${name} exceeds the maximum timer delay`);
  }
  return delay;
}

function intervalDelay(input: DurationInput): number {
  const delay = timeoutDelay(input, "ticker interval");
  if (delay <= 0) {
    throw new RangeError("@uniflowed/std/time: ticker interval must be greater than zero");
  }
  return delay;
}

function finite(value: number, name: string): number {
  if (!Number.isFinite(value)) {
    throw new RangeError(`@uniflowed/std/time: ${name} must be finite`);
  }
  return value;
}

const MAX_TIMER_DELAY = 2_147_483_647;
