import type { Instant, Duration } from "../../../types/interfaces/wasi-clocks-system-clock.d.ts";
declare function now(): Instant;
declare function getResolution(): Duration;
declare const _default: {
    now: typeof now;
    getResolution: typeof getResolution;
};
export default _default;
export type * from "../../../types/interfaces/wasi-clocks-system-clock.d.ts";
