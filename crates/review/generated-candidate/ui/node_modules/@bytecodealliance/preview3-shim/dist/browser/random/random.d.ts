declare function getRandomBytes(maxLen: bigint): Uint8Array;
declare function getRandomU64(): bigint;
declare const _default: {
    getRandomBytes: typeof getRandomBytes;
    getRandomU64: typeof getRandomU64;
};
export default _default;
export type * from "../../../types/interfaces/wasi-random-random.d.ts";
