<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';

  interface ViewComponentNodeData {
    name: string;
    componentType: string;
    entity?: string;
    fields: string[];
    filter?: string;
    events: Array<{ name: string; eventType: string }>;
    columns?: Array<{ label: string }>;
    inputFields?: Array<{ name: string }>;
    chart?: string;
  }

  let { data }: NodeProps<ViewComponentNodeData> = $props();

  const columnCount = $derived(data.columns?.length ?? 0);
  const fieldCount = $derived(data.inputFields?.length ?? data.fields?.length ?? 0);
</script>

<div class="view-component" class:typeList={data.componentType === 'list'} class:typeForm={data.componentType === 'form'} class:typeDetails={data.componentType === 'details'}>
  <div class="header">
    <span class="icon">
      {#if data.componentType === 'list'}📋
      {:else if data.componentType === 'form'}📝
      {:else if data.componentType === 'details'}📄
      {:else if data.componentType === 'search'}🔍
      {:else if data.componentType === 'tree'}🌳
      {:else}🧩
      {/if}
    </span>
    <span class="name">{data.name}</span>
  </div>

  {#if columnCount > 0 || fieldCount > 0 || data.chart}
    <div class="summary">
      {#if columnCount > 0}<span>{columnCount} columns</span>{/if}
      {#if fieldCount > 0}<span>{fieldCount} fields</span>{/if}
      {#if data.chart}<span>{data.chart} chart</span>{/if}
    </div>
  {/if}

  <div class="properties">
    <div class="prop"><span class="key">type</span> {data.componentType}</div>
    {#if data.entity}
      <div class="prop"><span class="key">data</span> {data.entity}</div>
    {/if}
    {#if data.fields && data.fields.length > 0}
      <div class="prop"><span class="key">fields</span> [{data.fields.join(', ')}]</div>
    {/if}
    {#if data.filter}
      <div class="prop"><span class="key">filter</span> {data.filter}</div>
    {/if}
  </div>

  {#if data.events && data.events.length > 0}
    <div class="events">
      {#each data.events as event}
        <div class="event-tag">
          ⚡ {event.eventType}
        </div>
      {/each}
    </div>
  {/if}
</div>

<Handle type="target" position={Position.Top} />
<Handle type="source" position={Position.Bottom} />

<style>
  .view-component {
    background: var(--vscode-editor-background, #1e1e1e);
    border: 1px solid var(--vscode-panel-border, #444);
    border-radius: 6px;
    padding: 10px;
    min-width: 200px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    font-size: 12px;
    color: var(--vscode-editor-foreground, #ccc);
  }
  .header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
  }
  .icon {
    font-size: 14px;
  }
  .name {
    font-weight: 600;
    font-size: 13px;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 10px;
    color: var(--vscode-descriptionForeground, #888);
    margin-bottom: 6px;
  }
  .properties {
    margin-bottom: 6px;
  }
  .prop {
    font-size: 11px;
    margin-bottom: 2px;
  }
  .key {
    color: var(--vscode-descriptionForeground, #888);
    margin-right: 4px;
  }
  .events {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding-top: 6px;
    border-top: 1px solid var(--vscode-panel-border, #333);
  }
  .event-tag {
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 3px;
    background: #d4a01722;
    color: #d4a017;
    cursor: pointer;
  }
</style>
