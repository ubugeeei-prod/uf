// @flow
"use client";
import { save } from "./_actions.js";
export component Page() { return <main><h1 id="mismatch">{typeof window === "undefined" ? "server value" : "client value"}</h1><button onClick={() => save("ok")}>Save</button></main>; }
