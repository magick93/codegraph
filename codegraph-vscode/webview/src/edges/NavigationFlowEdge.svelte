<script lang="ts">
  import { BaseEdge, getSmoothStepPath, type EdgeProps } from '@xyflow/svelte';

  interface NavigationFlowData {
    label?: string;
    parameterBinding?: Record<string, string>;
  }

  let { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, markerEnd }:
    EdgeProps<NavigationFlowData> = $props();

  let path = $derived(
    getSmoothStepPath({
      sourceX, sourceY, sourcePosition,
      targetX, targetY, targetPosition,
      borderRadius: 8,
    })
  );
</script>

<BaseEdge {id} {path} {markerEnd} class="navigation-flow" />
{#if data?.label}
  <edge-label>
    <div class="edge-label">
      <span class="event-name">{data.label}</span>
      {#if data.parameterBinding}
        <span class="binding">{JSON.stringify(data.parameterBinding)}</span>
      {/if}
    </div>
  </edge-label>
{/if}

<style>
  :global(.navigation-flow) {
    stroke: #3794ff;
    stroke-width: 2;
  }
  .edge-label {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    font-size: 11px;
    fill: var(--vscode-editor-foreground, #ccc);
    background: var(--vscode-editor-background, #1e1e1e);
    padding: 2px 6px;
    border-radius: 3px;
    border: 1px solid var(--vscode-panel-border, #333);
  }
  .event-name {
    font-weight: 500;
  }
  .binding {
    display: block;
    font-size: 10px;
    color: var(--vscode-descriptionForeground, #888);
    margin-top: 2px;
  }
</style>
