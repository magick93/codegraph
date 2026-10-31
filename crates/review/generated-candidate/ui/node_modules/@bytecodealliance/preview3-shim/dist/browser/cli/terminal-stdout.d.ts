import type { TerminalOutput } from "../../../types/interfaces/wasi-cli-terminal-stdout.d.ts";
declare function getTerminalStdout(): TerminalOutput | undefined;
declare const _default: {
    getTerminalStdout: typeof getTerminalStdout;
};
export default _default;
export type * from "../../../types/interfaces/wasi-cli-terminal-stdout.d.ts";
