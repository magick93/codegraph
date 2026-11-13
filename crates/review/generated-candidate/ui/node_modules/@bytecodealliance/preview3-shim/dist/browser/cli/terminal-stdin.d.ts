import type { TerminalInput } from "../../../types/interfaces/wasi-cli-terminal-stdin.d.ts";
declare function getTerminalStdin(): TerminalInput | undefined;
declare const _default: {
    getTerminalStdin: typeof getTerminalStdin;
};
export default _default;
export type * from "../../../types/interfaces/wasi-cli-terminal-stdin.d.ts";
