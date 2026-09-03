// @flow
call(alpha, beta, gamma, delta, epsilon, zeta, eta, theta, iota, kappa, lambda, mu, nu, xi, omicron);
callWithCallback(argument, (error, result) => {
  if (error) throw error;
  return result;
});
callWithFunction(function named() { return 1; }, second);
promise.then((value) => value.data).catch((error) => console.error(error)).finally(() => done());
const result = items.filter((item) => item.active).map((item) => item.id).slice(0, 10);
this.server = http.createServer(this.app).listen(port, () => console.log(`listening on ${port}`));
wrapper.find("SomeSelector").prop("children")(1).props().onChange({ target: { value: "x" } });
expect(screen.getByRole("button", { name: /submit/i })).toBeInTheDocument();
z.object({ name: z.string().min(1).max(100), email: z.string().email(), age: z.number().int().positive() });
useEffect(() => { subscribe(); return () => unsubscribe(); }, [subscribe, unsubscribe]);
useMemo(() => computeExpensiveValue(a, b), [a, b]);
const memoized = useCallback((event) => { setValue(event.target.value); }, []);
describe("suite", () => { it("does something really quite long in the description text here", async () => { await run(); }); });
it("has a name", function () { return true; });
setTimeout(() => { tick(); }, 1000);
fn(a)(b)(c);
curried(argumentOne)(argumentTwo)(argumentThree)(argumentFour)(argumentFive)(argumentSix)(argumentSeven);
const app = express().use(helmet()).use(cors()).use(express.json()).listen(3000);
object.property.another.deep.chain.of.members.that.is.very.long.indeed.longer.still.and.more.more;
Object.keys(map).forEach((key) => { delete map[key]; });
_.chain(users).filter("active").sortBy("age").map("name").value();
const shortChain = a.b().c();
const longFirst = someObject.someMethodWithALongName(argumentNumberOne, argumentNumberTwo).anotherMethod();
somePromise.then(function (result) { return result; });
array.map((x) => x * 2).filter(Boolean);
compose(withRouter, connect(mapStateToProps), withStyles(styles))(Component);
async function fetching() {
  const value = await fetchSomething(url, { method: "POST", headers: { "Content-Type": "application/json" }, body });
}
new Promise((resolve, reject) => { setTimeout(resolve, 100); });
require("module");
const lazy = require("./some/very/long/path/to/a/module/that/does/not/fit/on/one/line/at/all/really.js");
define(["dep"], function (dep) { return dep; });
foo(function () {}, bar);
foo((x) => {}, bar);
foo(bar, function () { return 1; });
foo({ a: 1, b: 2 }, [1, 2, 3]);
foo([1, 2, 3], { a: 1, b: 2 });
veryLongFunctionName(anotherLongArgumentName, { key: value, another: thing, more: stuff, evenMore: 1 });
veryLongFunctionName({ key: value, another: thing, more: stuff, evenMore: 1 }, anotherLongArgumentName);
render(<Component prop={value} other={thing} />, container);
