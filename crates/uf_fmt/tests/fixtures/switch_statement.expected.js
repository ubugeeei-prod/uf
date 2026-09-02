// @flow
function reduce(state: State, action: Action): State {
  switch (action.type) {
    case "increment":
      return { ...state, count: state.count + 1 };
    case "decrement": {
        const next = state.count - 1;
        return { ...state, count: next };
      }
    case "reset":
    case "clear":
      return initialState;
    default: {
        (action.type: empty);
        return state;
      }
  }
}

function nested(value) {
  switch (value) {
    case 1:
      switch (value % 2) {
        case 0:
          return "even";
        default:
          return "odd";
      }
    default:
      return "other";
  }
}
