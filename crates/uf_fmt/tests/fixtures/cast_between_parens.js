// @flow
// A cast that is the object of a member access or the callee of a call gets a
// line of its own between the parentheses it already wears — but only after a
// member lookup and an argument list have both failed to make room.
// react-devtools' renderer.js is the case, at four levels of indentation.

function f() {
  if (a) {
    if (b) {
      if (c) {
        const commitProfilingMetadata = (rootToCommitProfilingMetadataMap as any as CommitProfilingMetadataMap).get(currentRoot.id);
        // The argument list is enough here, so the cast stays on its line.
        const p8 = (rootToCommitProfilingMetadataMap as any as CommitProfilingMetadataMapLXY).get(aa);
        // And so is the lookup.
        const p1 = (rootToCommitProfilingMetadataMap as any as CommitProfilingMetadataMapLongXY).prop;
        const p2 = (rootToCommitProfilingMetadataMap as any as CommitProfilingMetadataMapLonXY)[keyy];
        // `new` is not a call for this purpose, so its arguments break instead.
        const p3 = new (rootToCommitProfilingMetadataMap as any as CommitProfilingMetadataMapLXY)(aa);
        // Nor is a cast that is nobody's object or callee.
        const p4 = !(rootToCommitProfilingMetadataMapXY as any as CommitProfilingMetadataMapLongeXY);
        const p9 = callSomething(rootToCommitProfilingMetadataMapLongerName as any as ProfilingMapx);
      }
    }
  }
}
