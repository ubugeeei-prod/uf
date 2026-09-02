// @flow
const inline = <span className="a">plain   text with   runs of spaces</span>;

const nested = (
  <article>
    <header>
      <h1>{title}</h1>
      {subtitle ? <h2>{subtitle}</h2> : null}
    </header>
    {items.map((item) => (
      <Row key={item.id} label={`${item.name} (${item.count})`}>
        {item.children.map((child) => <Cell key={child.id}>{child.value}</Cell>)}
      </Row>
    ))}
    <footer>
      copyright {year} — {owner}
    </footer>
  </article>
);

const fragment = <>{a}{b}</>;
const selfClosing = <br />;
const namespaced = <svg:rect data-testid="rect" width={10} />;
const spread = <Widget {...props} key="w" />;
const dotted = <Foo.Bar.Baz value={1} />;
