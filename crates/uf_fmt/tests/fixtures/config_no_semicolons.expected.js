// @flow
const a = 1
const b = compute()
let c = { key: "value" }

function f() {
  const local = a + b
  return local
}

for (let i = 0; i < 3; i++) {
  f()
}

while (poll());

class Widget {
  value = 1
  render() {
    return this.value
  }
}
