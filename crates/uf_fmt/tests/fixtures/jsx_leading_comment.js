// @flow
// A comment between the `(` and a JSX element. react-devtools' Button.js and
// rn-tester's SnapshotViewIOS.ios.js both write this, and both used to come
// back with the element wrapped twice: once by the parentheses the element
// asks for itself, once more by the assignment or the `return` breaking
// because it saw a leading own-line comment.

function Button({children, className, testName, ...rest}: Props): React.Node {
  let button = (
    // $FlowFixMe[cannot-spread-inexact] unsafe spread
    <button className={`${styles.Button} ${className}`} data-testname={testName} {...rest}>
      <span className={`${styles.ButtonContent} ${className}`} tabIndex={-1}>
        {children}
      </span>
    </button>
  );
  return button;
}

class Snapshot extends React.Component<Props> {
  render(): React.Node {
    return (
      // $FlowFixMe[incompatible-type] - Typing ReactNativeComponent revealed errors
      <RCTSnapshot style={style.snapshot} onSnapshotReady={onSnapshotReady} testIdentifier={id} />
    );
  }
}

const row = () => (
  // the element still gets to open its own parentheses on this line
  <Row key={item.id} value={item.value} />
);

const trailing = () => (
  <Row /> // and a trailing comment does not move it either
);

// A non-JSX right-hand side keeps the old answer: the comment forces the
// break after the operator, and there are no parentheses to double up.
const plain =
  // still on its own line
  someValue;
