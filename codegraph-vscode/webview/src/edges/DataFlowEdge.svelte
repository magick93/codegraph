<script lang="ts">
  import { BaseEdge, getBezierPath, type EdgeProps } from '@xyflow/svelte';

  interface DataFlowData {
    sourceParam?: string;
    targetParam?: string;
  }

  let { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, markerEnd }:
    EdgeProps<DataFlowData> = $props();

  let path = $derived(
    getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition })
  );
</script>

<BaseEdge
  {id}
  {path}
  {markerEnd}
  class="data-flow"
  style="stroke-dasharray: 6 4; stroke: #6b7280;"
/>

<style>
  :global(.data-flow) {
    stroke-width: 2;
  }
</style>
