import type { Result } from "../../../types/interfaces/wasi-cli-exit.d.ts";
declare function exit(status: Result<void, void>): void;
declare function exitWithCode(statusCode: number): void;
declare const _default: {
    exit: typeof exit;
    exitWithCode: typeof exitWithCode;
};
export default _default;
export type * from "../../../types/interfaces/wasi-cli-exit.d.ts";
