import "./server2.js";
import { n as writable, t as get } from "./index-server.js";
import "./index-server2.js";
import { r as getLocale } from "./runtime.js";
// src/lib/paraglide/messages/common_cancel.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_CancelInputs */
/**
* | output |
* | --- |
* | "Cancel" |
*
* @param {Common_CancelInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_cancel = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Cancel";
});
// src/lib/paraglide/messages/common_edit.js
/** @typedef {import('../runtime.js').LocalizedString} LocalizedString */
/** @typedef {{}} Common_EditInputs */
/**
* | output |
* | --- |
* | "Edit" |
*
* @param {Common_EditInputs} inputs
* @param {{ locale?: "en" }} options
* @returns {LocalizedString}
*/
var common_edit = ((inputs = {}, options = {}) => {
	options.locale ?? getLocale();
	return "Edit";
});
// node_modules/svelte-sonner/dist/internal/helpers.js
var isBrowser = typeof document !== "undefined";
/**
* A custom store that only allows setting/updating the value from the
* browser to avoid SSR data leaks. By defining this helper, we don't
* have to worry about checking for `isBrowser` in every place we
* mutate the various stores.
*
* This should only ever be initialized with an empty array or object,
* as otherwise the initial value will persist across requests.
*/
function clientWritable(initialValue) {
	const store = writable(initialValue);
	function set(value) {
		if (isBrowser) store.set(value);
	}
	function update(updater) {
		if (isBrowser) store.update(updater);
	}
	return {
		subscribe: store.subscribe,
		set,
		update
	};
}
// node_modules/svelte-sonner/dist/state.js
var toastsCounter = 0;
function createToastState() {
	const toasts = clientWritable([]);
	const heights = clientWritable([]);
	function addToast(data) {
		toasts.update((prev) => [data, ...prev]);
	}
	function create(data) {
		const { message, ...rest } = data;
		const id = typeof data?.id === "number" || data.id && data.id?.length > 0 ? data.id : toastsCounter++;
		const dismissable = data.dismissable === void 0 ? true : data.dismissable;
		const type = data.type === void 0 ? "default" : data.type;
		if (get(toasts).find((toast) => {
			return toast.id === id;
		})) toasts.update((prev) => prev.map((toast) => {
			if (toast.id === id) return {
				...toast,
				...data,
				id,
				title: message,
				dismissable,
				type,
				updated: true
			};
			return {
				...toast,
				updated: false
			};
		}));
		else addToast({
			...rest,
			id,
			title: message,
			dismissable,
			type
		});
		return id;
	}
	function dismiss(id) {
		if (id === void 0) {
			toasts.update((prev) => prev.map((toast) => ({
				...toast,
				dismiss: true
			})));
			return;
		}
		toasts.update((prev) => prev.map((toast) => toast.id === id ? {
			...toast,
			dismiss: true
		} : toast));
		return id;
	}
	function remove(id) {
		if (id === void 0) {
			toasts.set([]);
			return;
		}
		toasts.update((prev) => prev.filter((toast) => toast.id !== id));
		return id;
	}
	function message(message, data) {
		return create({
			...data,
			type: "default",
			message
		});
	}
	function error(message, data) {
		return create({
			...data,
			type: "error",
			message
		});
	}
	function success(message, data) {
		return create({
			...data,
			type: "success",
			message
		});
	}
	function info(message, data) {
		return create({
			...data,
			type: "info",
			message
		});
	}
	function warning(message, data) {
		return create({
			...data,
			type: "warning",
			message
		});
	}
	function loading(message, data) {
		return create({
			...data,
			type: "loading",
			message
		});
	}
	function promise(promise, data) {
		if (!data) return;
		let id = void 0;
		if (data.loading !== void 0) id = create({
			...data,
			promise,
			type: "loading",
			message: data.loading
		});
		const p = promise instanceof Promise ? promise : promise();
		let shouldDismiss = id !== void 0;
		p.then((response) => {
			if (response && typeof response.ok === "boolean" && !response.ok) {
				shouldDismiss = false;
				const message = typeof data.error === "function" ? data.error(`HTTP error! status: ${response.status}`) : data.error;
				create({
					id,
					type: "error",
					message
				});
			} else if (data.success !== void 0) {
				shouldDismiss = false;
				const message = typeof data.success === "function" ? data.success(response) : data.success;
				create({
					id,
					type: "success",
					message
				});
			}
		}).catch((error) => {
			if (data.error !== void 0) {
				shouldDismiss = false;
				const message = typeof data.error === "function" ? data.error(error) : data.error;
				create({
					id,
					type: "error",
					message
				});
			}
		}).finally(() => {
			if (shouldDismiss) {
				dismiss(id);
				id = void 0;
			}
			data.finally?.();
		});
		return id;
	}
	function custom(component, data) {
		const id = data?.id || toastsCounter++;
		create({
			component,
			id,
			...data
		});
		return id;
	}
	function removeHeight(id) {
		heights.update((prev) => prev.filter((height) => height.toastId !== id));
	}
	function setHeight(data) {
		if (get(heights).find((el) => el.toastId === data.toastId) === void 0) {
			heights.update((prev) => [data, ...prev]);
			return;
		}
		heights.update((prev) => prev.map((el) => {
			if (el.toastId === data.toastId) return data;
			else return el;
		}));
	}
	function reset() {
		toasts.set([]);
		heights.set([]);
	}
	return {
		create,
		addToast,
		dismiss,
		remove,
		message,
		error,
		success,
		info,
		warning,
		loading,
		promise,
		custom,
		removeHeight,
		setHeight,
		reset,
		toasts,
		heights
	};
}
var toastState = createToastState();
function toastFunction(message, data) {
	return toastState.create({
		message,
		...data
	});
}
var toast = Object.assign(toastFunction, {
	success: toastState.success,
	info: toastState.info,
	warning: toastState.warning,
	error: toastState.error,
	custom: toastState.custom,
	message: toastState.message,
	promise: toastState.promise,
	dismiss: toastState.dismiss,
	loading: toastState.loading
});
export { common_edit as n, common_cancel as r, toast as t };

//# sourceMappingURL=dist.js.map