<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';

  interface ActionNodeData {
    name: string;
    outcomes?: Array<{ type: string; target?: string }>;
  }

  let { data }: NodeProps<ActionNodeData> = $props();
</script>

<div class="action-node">
  <div class="header">
    <span class="icon">🛠️</span>
    <span class="name">{data.name}</span>
  </div>
  {#if data.outcomes && data.outcomes.length > 0}
    <div class="outcomes">
      {#each data.outcomes as outcome}
        <div class="outcome-tag" class:success={outcome.type === 'success'} class:error={outcome.type === 'error'}>
          {outcome.type}
        </div>
      {/each}
    </div>
  {/if}
</div>

<Handle type="target" position={Position.Top} />
<Handle type="source" position={Position.Bottom} />

<style>
  .action-node {
    background: var(--vscode-editor-background, #1e1e1e);
    border: 1px solid #6b7280;
    border-radius: 6px;
    padding: 8px 12px;
    min-width: 140px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    font-size: 12px;
    color: var(--vscode-editor-foreground, #ccc);
  }
  .header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
  }
  .icon {
    font-size: 14px;
  }
  .name {
    font-weight: 600;
    font-size: 13px;
  }
  .outcomes {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding-top: 4px;
    border-top: 1px solid var(--vscode-panel-border, #333);
  }
  .outcome-tag {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 3px;
    background: #6b728033;
    color: #9ca3af;
  }
  .outcome-tag.success {
    background: #10b98133;
    color: #10b981;
  }
  .outcome-tag.error {
    background: #ef444433;
    color: #ef4444;
  }
</style>
