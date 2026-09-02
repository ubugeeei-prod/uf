// @flow
import * as React from "react";

type Props = {|
  +title: string,
  +items: Array<string>,
  +onSelect?: ?(value: string) => void,
|};

hook useSelection(initial: string): [string, (next: string) => void] {
  const [value, setValue] = React.useState<string>(initial);
  const select = React.useCallback((next: string) => {
    setValue(next);
  }, []);
  return [value, select];
};

component ItemList(props: Props) renders React.Node {
  const [selected, select] = useSelection(props.items[0] ?? "");
  const total = props.items.length;

  return (
    <section className="list" data-total={total}>
      <h2>{props.title}</h2>
      <ul>
        {props.items.map((item, index) => (
          <li key={index} onClick={() => select(item)}>
            {item} {item === selected ? "(selected)" : ""}
          </li>
        ))}
      </ul>
    </section>
  );
}

export default ItemList;
