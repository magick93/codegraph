import type { Result, ErrorCode } from "../../../types/interfaces/wasi-cli-stdin.d.ts";
declare function readViaStream(): [ReadableStream<number>, Promise<Result<void, ErrorCode>>];
declare const _default: {
    readViaStream: typeof readViaStream;
};
export default _default;
export type * from "../../../types/interfaces/wasi-cli-stdin.d.ts";
