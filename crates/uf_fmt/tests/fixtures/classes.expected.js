// @flow
class Empty {}
class Point {
  x: number;
  y: number = 0;
  static origin: Point = new Point(0, 0);
  #secret: string = "hidden";
  static #count = 0;
  +readOnly: string;
  declare declared: number;

  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }

  get length(): number {
    return Math.sqrt(this.x * this.x + this.y * this.y);
  }
  set length(value: number) {
    this.scale(value / this.length);
  }
  static create(): Point {
    return new Point(1, 1);
  }
  async load() {
    await this.fetch();
  }
  *iterate() {
    yield this.x;
    yield this.y;
  }
  async *stream() {
    yield await this.next();
  }
  ["computed" + name]() {}
  static {
    Point.#count = 1;
  }
  #privateMethod() {
    return this.#secret;
  }
  scale(factor: number): this {
    this.x *= factor;
    return this;
  }
}
class Derived<T> extends Base<T> implements Interface, Other {}
class LongHeritage
  extends SomeVeryLongBaseClassNameThatIsLong
  implements FirstInterfaceName, SecondInterfaceName {}
class WithSuper extends (condition ? A : B) {}
class WithCall extends mixin(A, B) {}
const Anonymous = class {};
const Named = class Inner extends Outer {};
export default class extends Base {
  method() {}
}
class Members {
  a = 1;
  b = 2;

  c = 3;
  method() {}
  static property = "value";
  static method() {}
  aVeryLongPropertyName: SomeType<WithTypeArguments> = someInitializerFunction(
    withArguments,
    andMore,
  );
}
class Variance {
  +covariant: number;
  -contravariant: number;
}
class Generic<+T, -U, V: Bound = Default> {
  value: T;
}
