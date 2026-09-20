// @flow
export function register() {
  document.documentElement.dataset.instrumented = "true";
}

export function onError(error, context) {
  console.info("fixture browser error", context.source);
}

export function onNavigation(timing) {
  console.info("fixture navigation", timing.kind, timing.pathname);
}
