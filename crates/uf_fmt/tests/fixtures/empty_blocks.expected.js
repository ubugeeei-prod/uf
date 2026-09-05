// @flow
// Empty blocks. Prettier collapses a body that is expected to be empty
// sometimes and keeps open a block whose emptiness is worth seeing.

if (a) {
}
if (b) {
} else if (c) {
  d();
}
if (e) {
} else {
}
while (f) {}
for (;;) {}
function g() {}
try {
} catch (h) {}
const i = () => {};
class J {
  k() {}
}
switch (l) {
}
try {
} catch (a) {
} finally {
}
do {} while (b);
label: {
}
{
}
if (c) {
  // only a comment
}
class D {
  static {}
}
for (const a in b) {
}
for (const c of d) {
}
const e = { f() {}, get g() {}, set h(i) {} };
async function j() {}
function* k() {}
const l = async () => {};
class M {
  constructor() {}
  static n() {}
}
if (o) {
}
with (p) {
}
declare module "a" {
}
declare namespace b {
}
component C() {}
switch (d) {
  case 1: {
  }
}
