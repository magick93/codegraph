<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';

  interface ViewContainerNodeData {
    name: string;
    label: string;
    isLandmark: boolean;
    isModal: boolean;
    params: Array<{ name: string; typeRef: string }>;
    components: Array<{
      id: string;
      name: string;
      componentType: string;
      entity?: string;
    }>;
  }

  let { data }: NodeProps<ViewContainerNodeData> = $props();

  const hasParams = $derived(data.params && data.params.length > 0);
</script>

<div class="view-container" class:modal={data.isModal} class:landmark={data.isLandmark}>
  <div class="header">
    <span class="icon">🔲</span>
    <span class="name">{data.label || data.name}</span>
    {#if data.isLandmark}
      <span class="badge">★ landmark</span>
    {/if}
    {#if data.isModal}
      <span class="badge modal-badge">⊞ modal</span>
    {/if}
  </div>

  <div class="children">
    {#each data.components as comp}
      <div class="child-card" data-component-id={comp.id}>
        <span class="comp-type">{comp.componentType}</span>
        <span class="comp-name">{comp.name}</span>
        {#if comp.entity}
          <span class="comp-entity">[{comp.entity}]</span>
        {/if}
      </div>
    {/each}
  </div>

  {#if hasParams}
    <div class="params">
      params:
      {#each data.params as param}
        <span class="param">{param.name}: {param.typeRef}</span>
      {/each}
    </div>
  {/if}
</div>

<Handle type="target" position={Position.Top} />
<Handle type="source" position={Position.Bottom} />

<style>
  .view-container {
    background: var(--vscode-editor-background, #1e1e1e);
    border: 2px solid var(--vscode-focusBorder, #007acc);
    border-radius: 8px;
    padding: 12px;
    min-width: 280px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    font-size: 13px;
    color: var(--vscode-editor-foreground, #ccc);
  }
  .view-container.landmark {
    border-color: #d4a017;
  }
  .view-container.modal {
    border-style: dashed;
  }
  .header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--vscode-panel-border, #333);
  }
  .icon {
    font-size: 16px;
  }
  .name {
    font-weight: 600;
    font-size: 14px;
    flex: 1;
  }
  .badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 3px;
    background: #d4a01733;
    color: #d4a017;
  }
  .badge.modal-badge {
    background: #007acc33;
    color: #007acc;
  }
  .children {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 8px;
  }
  .child-card {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: var(--vscode-list-hoverBackground, #2a2d2e);
    border-radius: 4px;
    font-size: 12px;
  }
  .comp-type {
    font-weight: 500;
    color: var(--vscode-textLink-foreground, #3794ff);
  }
  .comp-name {
    font-weight: 500;
  }
  .comp-entity {
    color: var(--vscode-descriptionForeground, #888);
    font-style: italic;
  }
  .params {
    font-size: 11px;
    color: var(--vscode-descriptionForeground, #888);
    padding-top: 4px;
    border-top: 1px solid var(--vscode-panel-border, #333);
  }
</style>
