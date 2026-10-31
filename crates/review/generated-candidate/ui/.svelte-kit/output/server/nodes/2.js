import * as server from '../entries/pages/(app)/_layout.server.ts.js';

export const index = 2;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/(app)/_layout.svelte.js')).default;
export { server };
export const server_id = "src/routes/(app)/+layout.server.ts";
export const imports = ["_app/immutable/nodes/2.DzmJxRGf.js","_app/immutable/chunks/DAkl6bz8.js","_app/immutable/chunks/Bp7AKOp6.js","_app/immutable/chunks/9T-52J_U.js","_app/immutable/entry/payload.DSmR2FwN.js","_app/immutable/chunks/hiY2A2dd.js","_app/immutable/chunks/DDjABnAm.js","_app/immutable/chunks/4KhW4htH.js","_app/immutable/chunks/arvxp0OM.js","_app/immutable/chunks/DLNmc5u_.js","_app/immutable/chunks/CTeGUmiw.js"];
export const stylesheets = [];
export const fonts = [];
