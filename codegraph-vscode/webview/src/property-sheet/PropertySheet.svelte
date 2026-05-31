<script lang="ts">
  import type { Node, Edge } from '@xyflow/svelte';

  let { nodeId, edgeId, nodes, edges }: {
    nodeId: string | null;
    edgeId: string | null;
    nodes: Node[];
    edges: Edge[];
  } = $props();

  let selectedNode = $derived(nodes.find(n => n.id === nodeId));
  let selectedEdge = $derived(edges.find(e => e.id === edgeId));
  let element = $derived(selectedNode || selectedEdge);
</script>

<div class="property-sheet">
  <h3>Properties</h3>
  {#if element}
    <div class="field">
      <span class="label">ID</span>
      <span class="value">{element.id}</span>
    </div>
    <div class="field">
      <span class="label">Type</span>
      <span class="value">{element.type || element.data?.componentType || 'unknown'}</span>
    </div>
    {#if element.data}
      {#each Object.entries(element.data) as [key, value]}
        {#if key !== 'id' && typeof value === 'string'}
          <div class="field">
            <span class="label">{key}</span>
            <span class="value">{value}</span>
          </div>
        {/if}
      {/each}
    {/if}
  {:else}
    <p class="empty">Select an element to edit</p>
  {/if}
</div>

<style>
  .property-sheet {
    padding: 12px;
    flex: 1;
  }
  .property-sheet h3 {
    margin: 0 0 8px 0;
    font-size: 12px;
    text-transform: uppercase;
    color: var(--vscode-descriptionForeground, #888);
  }
  .field {
    margin-bottom: 8px;
  }
  .field .label {
    display: block;
    font-size: 11px;
    color: var(--vscode-descriptionForeground, #888);
    margin-bottom: 2px;
  }
  .field .value {
    font-size: 13px;
  }
  .empty {
    color: var(--vscode-disabledForeground, #aaa);
    font-style: italic;
  }
</style>
