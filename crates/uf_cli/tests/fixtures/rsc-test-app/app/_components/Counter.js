// @flow
"use client";
import { useEffect, useState } from "@uniflowed/react";
export default component Counter() {
  const [ready, setReady] = useState(false);
  const [count, setCount] = useState(0);
  useEffect(() => { setReady(true); }, []);
  return <button id="counter" data-ready={ready ? "yes" : "no"} onClick={() => setCount(count + 1)}>count: {count}</button>;
}
