// @flow
(function () {})();
(() => {})();
({}).toString();
({ a } = obj);
[a, b] = [b, a];
(class {}).name;
(async () => {})();
new (foo())();
new (foo.bar)();
new foo.bar();
new (foo().bar)();
(a ? b : c).d;
(a || b).c;
(a && b)();
(a + b) * c;
a + (b * c);
(a ** b) ** c;
a ** (b ** c);
(-a) ** b;
-(a ** b);
async function awaiting() {
  (await x).y;
}
function* generating() {
  (yield x);
}
typeof (a + b);
(typeof a) + b;
!(a && b);
(x = 1) + 2;
a = (b = c);
(a, b) ? c : d;
(function () {}).call(this);
(() => 1)();
(a => a)(1);
(x: any).foo;
((x: any): string);
x = (y: string);
(a ? b : c) ? d : e;
a ? (b ? c : d) : e;
a ? b : (c ? d : e);
(a || b) ?? c;
a ?? (b || c);
(a ?? b) && c;
(a in b) in c;
for (const x = (a in b); ;) {}
for (let i = (a in b) ? 1 : 2; ;) {}
(a.b) = c;
(a)();
(a).b;
(1).toString();
1.5.toString();
(-1).toString();
++(a);
(async function () {})();
(function* () {})();
const fnExpr = (function () {});
const cast = ((value: any): string);
export default (class {});
(a?.b)();
(a?.b).c;
(delete a.b)?.c;
(new Date()).getTime();
new (Date())();
(a < b) < c;
