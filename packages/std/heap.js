// @flow
//
// `@uniflowed/std/heap`: Go's `container/heap`, as a class with a comparator.
//
// A priority queue is the data structure JavaScript is missing that hurts most,
// because the two things people reach for instead are both wrong in a way that
// does not show up until the input grows:
//
// ```js
// queue.push(job);
// queue.sort(byPriority);      // O(n log n) per insert
// const next = queue.shift();  // O(n) per removal
// ```
//
// That is `O(n² log n)` to drain a queue that a heap drains in `O(n log n)`. It
// is correct, it is two lines, and it is why a scheduler that was fine with a
// hundred jobs falls over at ten thousand.
//
// # Why a class rather than Go's interface
//
// Go's `container/heap` asks the caller to implement `Len`, `Less`, `Swap`,
// `Push` and `Pop` and then calls them, which is how Go writes a generic
// container in a language that had no generics when the package was written. In
// Flow it costs five methods to say what one comparator says, and the
// comparator is the version that infers: `new Heap((a, b) => a.due - b.due)`
// gives a `Heap` of whatever `a` and `b` are, with nothing annotated.
//
// # A heap is not stable
//
// Two items that compare equal come out in an order nothing promises, and that
// order is not insertion order. This is a property of the structure and not
// something an implementation can fix — the sift operations move equal elements
// past each other. When ties matter, break them:
//
// ```js
// let arrived = 0;
// const queue = new Heap((a, b) => a.priority - b.priority || a.seq - b.seq);
// queue.push({ ...job, seq: (arrived += 1) });
// ```
//
// Which is two lines the caller writes rather than a counter every heap pays
// for. `Array.prototype.sort` *is* stable, so a full sort is still the right
// answer for a batch that is not a queue.
//
// # Cost
//
// `push` and `pop` are `O(log n)` with one comparison per level. [`peek`] and
// [`size`] are `O(1)`. [`heapify`] builds from an existing array in `O(n)` —
// genuinely linear, not `n log n` — which is the reason to have it rather than
// pushing in a loop. One array holds everything; there are no nodes.

/**
 * How two items are ordered: negative if `a` comes first, positive if `b` does.
 *
 * The same contract as `Array.prototype.sort`'s comparator, so a comparator
 * written for one works for the other. A comparator that is inconsistent — one
 * where `compare(a, b)` and `compare(b, a)` agree on a sign — does not corrupt
 * the heap's memory, but the order it produces means nothing.
 */
export type Compare<T> = (a: T, b: T) => number;

/**
 * A binary min-heap ordered by a comparator.
 *
 * "Min" is relative to the comparator: the item [`pop`] returns is the one that
 * compares less than every other, so a comparator of `(a, b) => b - a` is a
 * max-heap and no second class is needed.
 *
 * ```js
 * const queue = new Heap<Job>((a, b) => a.due - b.due);
 * queue.push(job);
 * const next = queue.pop();   // Job | void
 * ```
 *
 * `T` is inferred from the comparator, so the type argument above is
 * documentation rather than a requirement.
 */
export class Heap<T> {
  #items: Array<T>;
  #compare: Compare<T>;

  /**
   * A heap ordered by `compare`, holding `initial` if any is given.
   *
   * `initial` is copied and then heapified in place, which is linear — see
   * [`heapify`], the name that says so at the call site.
   */
  constructor(compare: Compare<T>, initial?: $ReadOnlyArray<T>) {
    this.#compare = compare;
    this.#items = initial == null ? [] : initial.slice();
    for (let index = (this.#items.length >> 1) - 1; index >= 0; index -= 1) {
      this.#down(index);
    }
  }

  /** How many items are in the heap. */
  size(): number {
    return this.#items.length;
  }

  /** Whether the heap holds nothing. */
  isEmpty(): boolean {
    return this.#items.length === 0;
  }

  /** Add `item`. `O(log n)`. */
  push(item: T): void {
    this.#items.push(item);
    this.#up(this.#items.length - 1);
  }

  /**
   * Remove and return the least item, or `undefined` when the heap is empty.
   *
   * `T | void` rather than throwing, because "is there anything left" is the
   * question a drain loop asks on every iteration and an exception is a poor
   * way to answer a question that is asked in a condition.
   */
  pop(): T | void {
    const items = this.#items;
    if (items.length === 0) {
      return undefined;
    }
    const top = items[0];
    // Read the last item before shortening the array rather than taking what
    // `pop` hands back: on a `Heap<T | void>` the returned value is
    // indistinguishable from "there was nothing there", and a heap whose
    // elements may legitimately be `undefined` would leave a hole at the root.
    const last = items[items.length - 1];
    items.pop();
    if (items.length > 0) {
      items[0] = last;
      this.#down(0);
    }
    return top;
  }

  /** The least item without removing it, or `undefined`. `O(1)`. */
  peek(): T | void {
    return this.#items.length === 0 ? undefined : this.#items[0];
  }

  /**
   * Replace the least item with `item` in one sift, and return what was there.
   *
   * A `pop` followed by a `push` does two sifts and briefly shrinks the array;
   * this does one, which is worth having in the loop it exists for — merging
   * `k` sorted streams, where every step takes the smallest head and puts back
   * the next element of the stream it came from.
   *
   * On an empty heap this is a plain `push`, and returns `undefined`.
   */
  replace(item: T): T | void {
    const items = this.#items;
    if (items.length === 0) {
      items.push(item);
      return undefined;
    }
    const top = items[0];
    items[0] = item;
    this.#down(0);
    return top;
  }

  /**
   * Every item, in no particular order, as a copy.
   *
   * The heap's own array is a heap and not a sorted list — only the first
   * element is guaranteed to be anything — so this is for inspection, and
   * anything that needs them in order should [`drain`] instead.
   */
  toArray(): $ReadOnlyArray<T> {
    return this.#items.slice();
  }

  /** Remove everything. */
  clear(): void {
    this.#items = [];
  }

  /**
   * Every item in order, emptying the heap as it goes.
   *
   * A generator, so a caller that stops early — `for (const job of q.drain())
   * { if (job.due > now) break; }` — pays only for what it took, and the rest
   * stays in the heap. That is the difference from sorting: a drain that reads
   * the first ten of a million costs `O(n + 10 log n)`, not `O(n log n)`.
   */
  *drain(): Generator<T, void, void> {
    let next = this.pop();
    while (next !== undefined) {
      yield next;
      next = this.pop();
    }
  }

  /** Move the item at `at` up until its parent is not greater than it. */
  #up(at: number): void {
    const items = this.#items;
    let index = at;
    const item = items[index];
    while (index > 0) {
      const parent = (index - 1) >> 1;
      if (this.#compare(item, items[parent]) >= 0) {
        break;
      }
      items[index] = items[parent];
      index = parent;
    }
    items[index] = item;
  }

  /** Move the item at `at` down until both children are not less than it. */
  #down(at: number): void {
    const items = this.#items;
    const length = items.length;
    let index = at;
    const item = items[index];
    for (;;) {
      const left = index * 2 + 1;
      if (left >= length) {
        break;
      }
      const right = left + 1;
      // The smaller child, because swapping with the larger one would leave it
      // above its own sibling.
      const child = right < length && this.#compare(items[right], items[left]) < 0 ? right : left;
      if (this.#compare(items[child], item) >= 0) {
        break;
      }
      items[index] = items[child];
      index = child;
    }
    items[index] = item;
  }
}

/**
 * Build a heap from items already in hand, in `O(n)`.
 *
 * Sifting down from the middle of the array costs linear time in total, because
 * most of the array is leaves and a leaf sifts zero levels. Pushing the same
 * items one at a time costs `O(n log n)`, and the difference is the reason
 * Go's `heap.Init` exists.
 *
 * ```js
 * const queue = heapify(jobs, (a, b) => a.due - b.due);
 * ```
 *
 * `items` is copied, so the caller's array is untouched and the heap owns what
 * it holds.
 */
export function heapify<T>(items: $ReadOnlyArray<T>, compare: Compare<T>): Heap<T> {
  return new Heap(compare, items);
}
