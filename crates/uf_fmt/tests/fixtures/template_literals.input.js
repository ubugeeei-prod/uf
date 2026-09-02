// @flow
const simple=`no interpolation at all`;
const one=`value: ${ value }`;
const nested=`outer ${ `inner ${ deep }  end` } tail`;
const deep=`a${ b`c${ d }e` }f`;
const withObject=`${ {key:1}.key }`;
const multiline=`
  keeps   its own
    indentation and ${ spacing }
`;
const tagged=html`<b class="x">${ content }</b>`;
const quotes=`he said 'hi' and "bye"`;
const escaped=`a \${ not an interpolation } b`;
const division=`${ a / b }`;
const regex=`${ /ab+/.test(c) }`;
