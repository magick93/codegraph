import { createMachine } from 'xstate';

/**
 * The PayRun lifecycle, authored in TypeScript/XState (pherolang#388) —
 * the fixture for issue #815's machine kernel. The parser is
 * fail-closed: no inline guards/actions/entry/exit, no identifier refs,
 * no non-literal scalars. The states are literals of the bound
 * `PayRunStatus` enum.
 */
export const payrunMachine = createMachine({
  id: 'payrun',
  initial: 'Draft',
  states: {
    Draft: {
      on: {
        CALCULATE: 'Calculated',
      },
    },
    Calculated: { type: 'final' },
  },
});
