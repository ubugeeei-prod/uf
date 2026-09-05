// @flow
// On the right of an `=`, a `.property` whose object is a call with arguments
// stays glued to the call: the call's argument list is the better place to
// break, and it is measured first, so leaving the lookup breakable hands it an
// escape that costs a line and saves nothing. prepack's arraybuffer.js is the
// file that noticed.

function DetachArrayBuffer(realm: Realm, arrayBuffer: ObjectValue) {
  Properties.ThrowIfInternalSlotNotWritable(realm, arrayBuffer, "$ArrayBufferData").$ArrayBufferData = null;
  let block = Properties.ThrowIfInternalSlotNotWritable(realm, arrayBuffer, "$ArrayBufferData").$ArrayBufferData;
  return block;
}

// A declarator asks the same question an assignment does, and a plain call
// callee reaches the rule just as a chain does.
let a2 = fn(aaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbbbbbbbbbbbbXYZ).someVeryLongPropertyNameHereOk;

// A member between the call and the `=` is what stands to break, so it does,
// and the tail is not glued to it.
let a4 = obj.fn(aaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbbbbbbbbXYZ).prop.someVeryLongPropertyName;

// A chain long enough to have an expanded form is the other half of the same
// rule: the tail rides on the last group rather than taking a line of its own.
let c2 = object.methodOne(argument).methodTwo(argument).methodThree(argument).someLongPropertyNameXY;

// And the same chain with one more lookup in between.
let d1 = object.methodOne(argument).methodTwo(argument).methodThree(argument).prop.someLongPropXYZWV;
