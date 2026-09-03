// @flow
"use strict";

import { a, b } from "./mod";

let x = 1, y = 2;
var z;
const { p, q: renamed = 3, ...rest } = obj;
const [first, , third = 4, ...others] = list;

if (x) y();
if (x) { y(); } else if (z) { w(); } else { v(); }
if (a) b(); else c();

for (let i = 0; i < 10; i++) { total += i; }
for (;;) {}
for (const key in object) if (has(object, key)) keys.push(key);
for (const item of items) process(item);
async function streaming() {
  for await (const chunk of stream) { consume(chunk); }
}

while (running) tick();
do { step(); } while (!done);

label: for (const row of rows) { for (const cell of row) { if (cell === 0) continue label; if (cell < 0) break label; } }

switch (kind) {
  case "a":
  case "b": {
    handle(kind);
    break;
  }
  case "c":
    fallthrough();
  default:
    unknown();
}

try { risky(); } catch (error) { report(error); } finally { cleanup(); }
try { risky(); } catch { ignore(); }

throw new Error("boom");

function early(value) { if (!value) return; return value * 2; }
function longReturn() { return someCondition && anotherCondition && yetAnotherCondition && oneMoreConditionHere; }

debugger;
;
{ nested(); }
