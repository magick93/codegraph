declare function getInsecureRandomBytes(maxLen: bigint): Uint8Array;
declare function getInsecureRandomU64(): bigint;
declare const _default: {
    getInsecureRandomBytes: typeof getInsecureRandomBytes;
    getInsecureRandomU64: typeof getInsecureRandomU64;
};
export default _default;
export type * from "../../../types/interfaces/wasi-random-insecure.d.ts";
