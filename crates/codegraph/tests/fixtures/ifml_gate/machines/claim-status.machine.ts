import { createMachine } from 'xstate';

/**
 * The Claim lifecycle, authored in TypeScript/XState (pherolang#388) —
 * the fixture machine for issue #817's read-model contract. The parser is
 * fail-closed: no inline guards/actions/entry/exit, no identifier refs,
 * no non-literal scalars. The states are literals of the bound
 * `ClaimStatus` enum and MUST agree with the claims workflow config (the
 * two-representation cross-check, issue #816).
 */
export const claimStatusMachine = createMachine({
  id: 'claimstatus',
  initial: 'draft',
  states: {
    draft: {
      on: {
        SUBMIT: 'submitted',
      },
    },
    submitted: { type: 'final' },
  },
});
