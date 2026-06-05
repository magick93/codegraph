<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';

  interface EventNodeData {
    name: string;
    eventType: string;
    params: string[];
  }

  let { data }: NodeProps<EventNodeData> = $props();

  const eventIcons: Record<string, string> = {
    select: '👆',
    submit: '➡️',
    click: '🖱️',
    load: '📥',
    save: '💾',
    cancel: '✖️',
    delete: '🗑️',
    confirm: '✅',
    back: '↩️',
  };
</script>

<div class="event-node" class:eventSelect={data.eventType === 'select'} class:eventSubmit={data.eventType === 'submit'} class:eventClick={data.eventType === 'click'}>
  <div class="header">
    <span class="icon">{eventIcons[data.eventType] || '⚡'}</span>
    <span class="name">{data.eventType}</span>
  </div>
  {#if data.params && data.params.length > 0}
    <div class="params">({data.params.join(', ')})</div>
  {/if}
</div>

<Handle type="target" position={Position.Left} />
<Handle type="source" position={Position.Right} />

<style>
  .event-node {
    background: var(--vscode-editor-background, #1e1e1e);
    border: 1px solid #d4a017;
    border-radius: 12px;
    padding: 6px 12px;
    min-width: 80px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    font-size: 12px;
    color: var(--vscode-editor-foreground, #ccc);
    text-align: center;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
  }
  .icon {
    font-size: 12px;
  }
  .name {
    font-weight: 500;
  }
  .params {
    font-size: 10px;
    color: var(--vscode-descriptionForeground, #888);
    margin-top: 2px;
  }
</style>
