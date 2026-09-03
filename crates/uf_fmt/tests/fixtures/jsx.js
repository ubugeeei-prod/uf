// @flow
const simple = <div />;
const withAttrs = <div className="container" id={id} data-value={1} disabled />;
const spreadAttrs = <Component {...props} extra="x" {...rest} />;
const children = <ul><li>one</li><li>two</li></ul>;
const text = <p>Hello, world!</p>;
const mixedText = <p>Hello, {name}! You have {count} new {count === 1 ? "message" : "messages"}.</p>;
const longText = <p>This is a very long paragraph of text that will need to be wrapped across multiple lines by the formatter because it is long.</p>;
const spaces = <span>a{" "}{b}{" "}c</span>;
const leadingSpace = <span> {value}</span>;
const trailingSpace = <span>{value} </span>;
const fragment = <><First /><Second /></>;
const conditional = cond ? <A /> : <B />;
const conditionalLong = someCondition ? <FirstComponent withProp={value} /> : <SecondComponent withOtherProp={other} />;
const logical = show && <Thing />;
const nested = (
  <Outer>
    <Inner prop="value">
      {items.map((item) => <Item key={item.id} {...item} />)}
    </Inner>
  </Outer>
);
function Component() {
  return <div className="wrapper"><Header title={title} subtitle={subtitle} onClick={handleClick} /><Body>{children}</Body></div>;
}
const member = <Namespace.Component prop={1} />;
const namespaced = <svg:rect xlink:href="#id" />;
const emptyExpression = <div>{/* comment */}</div>;
const multiline = <div
  a="1"
  b="2"
/>;
const stringWithQuotes = <input value='He said "hi"' placeholder="it's" />;
const entities = <p>&nbsp;&amp;&lt;</p>;
const newlineAttr = <p title="line one
line two" />;
const arrowChild = <List>{(item) => <Row item={item} />}</List>;
const callChild = <List>{items.map(render)}</List>;
const template = <div>{`template ${literal}`}</div>;
const generic = <Component<Props> prop={1} />;
const veryLongAttributeList = <Component firstProperty={firstValue} secondProperty={secondValue} thirdProperty={thirdValue} />;
const parenthesized = <div>
  text
</div>;
const returned = () => <div>
  <span />
</div>;
const inArray = [<A key="a" />, <B key="b" />];
const inCall = render(<App />, root);
const blankLines = (
  <div>
    <A />

    <B />
  </div>
);
const textAroundElements = <p>before <b>bold</b> after</p>;
const selfClosingText = <p>text <br /> more</p>;
const longWordsMultilineText = (
  <p>
    Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
  </p>
);
