import { n as noop, t as disallow_on_server } from "./functions.js";
// node_modules/@sveltejs/kit/src/runtime/app/navigation/server.js
var afterNavigate = noop;
var beforeNavigate = noop;
disallow_on_server("disableScrollHandling", "()");
var goto = disallow_on_server("goto");
disallow_on_server("invalidate");
var invalidateAll = disallow_on_server("invalidateAll", "()");
disallow_on_server("refreshAll", "()");
disallow_on_server("preloadCode");
disallow_on_server("preloadData");
disallow_on_server("pushState");
disallow_on_server("replaceState");
export { invalidateAll as i, beforeNavigate as n, goto as r, afterNavigate as t };

//# sourceMappingURL=navigation.js.map