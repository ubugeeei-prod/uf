// @flow
"use client";
import { useEffect, useState } from "@uniflowed/react";
export component Counter() {
  const [count, setCount] = useState(0);
  useEffect(() => { document.getElementById("counter")?.setAttribute("data-ready", "yes"); }, []);
  return <button type="button" id="counter" data-ready="no" onClick={() => setCount(count + 1)}>count: {count}</button>;
}
