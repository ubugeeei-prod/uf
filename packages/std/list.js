// @flow
//
// `@uniflowed/std/list`: Go's `container/list`, with stable element handles.
//
// JavaScript arrays are a good default container, but they make one specific
// workload expensive: "I already know the item, now move it to the front" is
// `O(n)` because the array has to find it and then shift everything between the
// old and new positions. That is the hot path of an LRU cache, a scheduler that
// reprioritises existing work, and any queue that splices from the middle.
//
// This module is the small linked-list primitive for that case. A `List` gives
// back an `Element` when a value is inserted, and that element stays a handle to
// its node until it is removed. Moving or removing by handle is `O(1)`.
//
// # Handles belong to one list
//
// The cost of a stable handle is that it has identity. A value can appear in a
// list many times; the element is the thing that says "this occurrence". To
// keep bugs loud, every method that receives an element checks that it still
// belongs to this list. Reusing a removed element or an element from another
// list throws a `RangeError` instead of corrupting two lists at once.
//
// # Cost
//
// `pushFront`, `pushBack`, `insertBefore`, `insertAfter`, `remove` and every
// `move*` method are `O(1)`. Iteration is linear, and yields the list's values
// rather than its elements because the common case is reading contents; callers
// that need handles keep the values returned by the insertion methods.

/** A stable handle to one occurrence of a value inside a `List`. */
export class Element<T> {
  _list: List<T> | null;
  _previous: Element<T> | null;
  _next: Element<T> | null;
  _value: T;

  /**
   * A detached element.
   *
   * `List` methods create the useful elements. The constructor is public
   * because JavaScript classes cannot export a type without a constructor; an
   * element created directly has a value but belongs to no list.
   */
  constructor(value: T) {
    this._list = null;
    this._previous = null;
    this._next = null;
    this._value = value;
  }

  /** The value this element carries. */
  value(): T {
    return this._value;
  }

  /** The next element, or `undefined` at the back or after removal. */
  next(): Element<T> | void {
    return this._list == null ? undefined : (this._next ?? undefined);
  }

  /** The previous element, or `undefined` at the front or after removal. */
  previous(): Element<T> | void {
    return this._list == null ? undefined : (this._previous ?? undefined);
  }
}

/**
 * A doubly linked list with stable element handles.
 *
 * ```js
 * const recency = new List<string>();
 * const inbox = recency.pushFront("inbox");
 * recency.pushFront("home");
 * recency.moveToFront(inbox);
 * [...recency.values()]; // ["inbox", "home"]
 * ```
 */
export class List<T> {
  #front: Element<T> | null = null;
  #back: Element<T> | null = null;
  #size: number = 0;

  /** A list holding `initial`, in order, if any values are given. */
  constructor(initial?: $ReadOnlyArray<T>) {
    if (initial != null) {
      for (const value of initial) {
        this.pushBack(value);
      }
    }
  }

  /** How many elements are in the list. */
  size(): number {
    return this.#size;
  }

  /** Whether the list holds nothing. */
  isEmpty(): boolean {
    return this.#size === 0;
  }

  /** The front element, or `undefined` when the list is empty. */
  front(): Element<T> | void {
    return this.#front ?? undefined;
  }

  /** The back element, or `undefined` when the list is empty. */
  back(): Element<T> | void {
    return this.#back ?? undefined;
  }

  /** Add `value` at the front and return its element handle. */
  pushFront(value: T): Element<T> {
    const element = new Element(value);
    this.#insertBetween(element, null, this.#front);
    return element;
  }

  /** Add `value` at the back and return its element handle. */
  pushBack(value: T): Element<T> {
    const element = new Element(value);
    this.#insertBetween(element, this.#back, null);
    return element;
  }

  /** Add `value` immediately before `mark`. */
  insertBefore(value: T, mark: Element<T>): Element<T> {
    this.#assertMember(mark);
    const element = new Element(value);
    this.#insertBetween(element, mark._previous, mark);
    return element;
  }

  /** Add `value` immediately after `mark`. */
  insertAfter(value: T, mark: Element<T>): Element<T> {
    this.#assertMember(mark);
    const element = new Element(value);
    this.#insertBetween(element, mark, mark._next);
    return element;
  }

  /** Remove `element` and return its value. */
  remove(element: Element<T>): T {
    this.#assertMember(element);
    this.#unlink(element);
    return element.value();
  }

  /** Move `element` to the front. */
  moveToFront(element: Element<T>): void {
    this.#assertMember(element);
    if (element === this.#front) {
      return;
    }
    this.#unlink(element);
    this.#insertBetween(element, null, this.#front);
  }

  /** Move `element` to the back. */
  moveToBack(element: Element<T>): void {
    this.#assertMember(element);
    if (element === this.#back) {
      return;
    }
    this.#unlink(element);
    this.#insertBetween(element, this.#back, null);
  }

  /** Move `element` immediately before `mark`. Both must belong to this list. */
  moveBefore(element: Element<T>, mark: Element<T>): void {
    this.#assertMember(element);
    this.#assertMember(mark);
    if (element === mark || element._next === mark) {
      return;
    }
    this.#unlink(element);
    this.#insertBetween(element, mark._previous, mark);
  }

  /** Move `element` immediately after `mark`. Both must belong to this list. */
  moveAfter(element: Element<T>, mark: Element<T>): void {
    this.#assertMember(element);
    this.#assertMember(mark);
    if (element === mark || element._previous === mark) {
      return;
    }
    this.#unlink(element);
    this.#insertBetween(element, mark, mark._next);
  }

  /** Remove everything and detach every element. */
  clear(): void {
    let element = this.#front;
    while (element != null) {
      const next = element._next;
      element._list = null;
      element._previous = null;
      element._next = null;
      element = next;
    }
    this.#front = null;
    this.#back = null;
    this.#size = 0;
  }

  /** The list's values from front to back. */
  *values(): Generator<T, void, void> {
    let element = this.#front;
    while (element != null) {
      const next = element._next;
      yield element.value();
      element = next;
    }
  }

  /** Iterate values from front to back. */
  [Symbol.iterator](): Iterator<T> {
    return this.values();
  }

  /** Return the values as an array, preserving order. */
  toArray(): $ReadOnlyArray<T> {
    return [...this.values()];
  }

  #assertMember(element: Element<T>): void {
    if (element._list !== this) {
      throw new RangeError("@uniflowed/std/list: element does not belong to this list");
    }
  }

  #insertBetween(element: Element<T>, previous: Element<T> | null, next: Element<T> | null): void {
    element._list = this;
    element._previous = previous;
    element._next = next;
    if (previous == null) {
      this.#front = element;
    } else {
      previous._next = element;
    }
    if (next == null) {
      this.#back = element;
    } else {
      next._previous = element;
    }
    this.#size += 1;
  }

  #unlink(element: Element<T>): void {
    const previous = element._previous;
    const next = element._next;
    if (previous == null) {
      this.#front = next;
    } else {
      previous._next = next;
    }
    if (next == null) {
      this.#back = previous;
    } else {
      next._previous = previous;
    }
    element._list = null;
    element._previous = null;
    element._next = null;
    this.#size -= 1;
  }
}
