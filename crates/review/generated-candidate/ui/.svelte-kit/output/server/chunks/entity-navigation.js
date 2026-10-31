import { n as writable, t as get } from "./index-server.js";
import { n as beforeNavigate } from "./navigation.js";
var entityNavigation = writable({
	breadcrumbs: [],
	parentContext: null,
	dirtyPages: {}
});
function pushChild(entry, parentCtx) {
	entityNavigation.update((s) => ({
		...s,
		breadcrumbs: [...s.breadcrumbs, entry],
		parentContext: parentCtx
	}));
}
function hasDirtyPages() {
	const state = get(entityNavigation);
	return Object.values(state.dirtyPages).some(Boolean);
}
function installDirtyGuard() {
	beforeNavigate(({ cancel }) => {
		if (hasDirtyPages()) {
			if (!confirm("You have unsaved changes. Are you sure you want to leave?")) cancel();
		}
	});
}
export { installDirtyGuard as n, pushChild as r, entityNavigation as t };

//# sourceMappingURL=entity-navigation.js.map