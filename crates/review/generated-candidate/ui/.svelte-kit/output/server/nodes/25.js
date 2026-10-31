import * as server from '../entries/pages/(app)/common/distribution-base/_page.server.ts.js';

export const index = 25;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/(app)/common/distribution-base/_page.svelte.js')).default;
export { server };
export const server_id = "src/routes/(app)/common/distribution-base/+page.server.ts";
export const imports = ["_app/immutable/nodes/25.gZBtuRKA.js","_app/immutable/chunks/DAkl6bz8.js","_app/immutable/chunks/hiY2A2dd.js","_app/immutable/entry/payload.DSmR2FwN.js","_app/immutable/chunks/9T-52J_U.js","_app/immutable/chunks/Bp7AKOp6.js","_app/immutable/chunks/Cd8fNcXw.js","_app/immutable/chunks/CTeGUmiw.js","_app/immutable/chunks/CCEmCDaa.js","_app/immutable/chunks/DLNmc5u_.js","_app/immutable/chunks/CbXdPKy5.js","_app/immutable/chunks/CxuP3-J2.js","_app/immutable/chunks/DjH5GJ84.js","_app/immutable/chunks/CUYSWbUk.js"];
export const stylesheets = [];
export const fonts = [];
