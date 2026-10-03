// @flow

import { stylex } from "@uniflowed/stylex";

/**
 * Shared controls. Defined here, not in a client module, so server components
 * can read the compiled class names. `form-ui.client.js` re-exports them.
 */

// Flow cannot serialize this generic factory call into the module signature.
// $FlowFixMe[signature-verification-failure]
export const styles = stylex.create({
  button: {
    display       : "inline-flex",
    alignItems    : "center",
    justifyContent: "center",
    gap           : "10px",
    border        : "1px solid #e3e3e3",
    borderRadius  : "6px",
    fontSize      : "12px",
    fontWeight    : "550",
    minHeight     : "38px",
    background    : "#fff",
    whiteSpace    : "nowrap",
    paddingTop    : "9px",
    paddingRight  : "15px",
    paddingBottom : "9px",
    paddingLeft   : "15px",
  },
  primary: {
    background : "var(--accent)",
    borderColor: "var(--accent)",
    color      : "#fff",
    ":hover": {
      background : "#131313",
      borderColor: "#131313",
    },
  },
  secondary: {
    background: "#fff",
    ":hover": {
      background: "var(--subtle)",
    },
  },
  textLink: {
    fontSize           : "12px",
    fontWeight         : "550",
    textDecoration     : "underline",
    textUnderlineOffset: "3px",
  },
  field: {
    display     : "grid",
    gap         : "8px",
    fontSize    : "12px",
    color       : "#5f5f5f",
    marginBottom: "19px",
  },
  fieldLabel: {
    fontWeight: "550",
  },
  fieldCopy: {
    fontSize  : "11px",
    lineHeight: "1.6",
    color     : "var(--muted)",
  },
  fieldAlert: {
    fontSize: "11px",
    color   : "#b13749",
  },
  fieldControl: {
    width        : "100%",
    border       : "1px solid #e2e2e2",
    borderRadius : "6px",
    fontSize     : { default: "13px", "@media (max-width: 760px)": "16px" },
    color        : "var(--ink)",
    lineHeight   : "1.5",
    background   : "#ffffffc2",
    borderColor  : "#d7d7d7",
    paddingTop   : "10px",
    paddingRight : "12px",
    paddingBottom: "10px",
    paddingLeft  : "12px",
    ":is([aria-invalid=true])": {
      borderColor: "#bf6674",
    },
  },
  formStatus: {
    fontSize    : "12px",
    lineHeight  : "1.6",
    display     : "flex",
    gap         : "7px",
    alignItems  : "center",
    overflowWrap: "anywhere",
    ":not(:empty)": {
      paddingTop   : "10px",
      paddingBottom: "10px",
    },
  },
  formStatusError: {
    color: "#b13749",
  },
  formStatusSuccess: {
    color: "#525252",
  },
  fieldError: {
    color     : "#b13749",
    display   : "block",
    fontSize  : "11px",
    lineHeight: "1.6",
    ":empty": {
      display: "none",
    },
  },
});
