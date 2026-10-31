import type { IpAddress } from "../../../types/interfaces/wasi-sockets-ip-name-lookup.d.ts";
declare function resolveAddresses(name: string): Promise<Array<IpAddress>>;
declare const _default: {
    resolveAddresses: typeof resolveAddresses;
};
export default _default;
export type * from "../../../types/interfaces/wasi-sockets-ip-name-lookup.d.ts";
