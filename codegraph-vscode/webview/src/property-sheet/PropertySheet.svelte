<script lang="ts">
  import type { Node, Edge } from '@xyflow/svelte';
  import type { CodegenConfig } from '../types';
  import { SyncClient } from '../sync';

  let { nodeId, edgeId, nodes, edges, codegenConfig }: {
    nodeId: string | null;
    edgeId: string | null;
    nodes: Node[];
    edges: Edge[];
    codegenConfig: CodegenConfig | null;
  } = $props();

  let selectedNode = $derived(nodes.find(n => n.id === nodeId));
  let selectedEdge = $derived(edges.find(e => e.id === edgeId));
  let element = $derived(selectedNode || selectedEdge);

  const sync = new SyncClient();

  function onToggle(framework: string, enabled: boolean) {
    sync.sendCodegenToggle(framework, enabled);
  }

  function onGenerate() {
    sync.sendCodegenRun();
  }

  function formatTimeAgo(iso: string): string {
    const diff = Date.now() - new Date(iso).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  }
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

  {#if codegenConfig}
    <div class="section">
      <h3>⚡ Code Generation</h3>
      <div class="framework-list">
        {#each codegenConfig.frameworks as fw}
          <label class="framework-item" class:disabled={!fw.available}>
            <input type="checkbox" checked={codegenConfig.targets.includes(fw.id)}
              onchange={(e) => onToggle(fw.id, e.currentTarget.checked)}
              disabled={!fw.available} />
            <span class="fw-label">{fw.label}</span>
            <span class="fw-desc">{fw.description}</span>
          </label>
        {/each}
      </div>
      <button class="generate-btn" onclick={() => onGenerate()}>▶ Generate All</button>
      {#if codegenConfig.lastRun}
        <span class="last-run">Last run: {formatTimeAgo(codegenConfig.lastRun)}</span>
      {/if}
    </div>
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
  .section {
    margin-top: 16px;
    border-top: 1px solid var(--vscode-panel-border, #ccc);
    padding-top: 12px;
  }
  .section h3 {
    margin: 0 0 8px 0;
    font-size: 12px;
    text-transform: uppercase;
    color: var(--vscode-descriptionForeground, #888);
  }
  .framework-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
  }
  .framework-item {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    cursor: pointer;
  }
  .framework-item.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .fw-label {
    font-weight: 600;
    min-width: 80px;
  }
  .fw-desc {
    color: var(--vscode-descriptionForeground, #888);
    font-size: 11px;
  }
  .generate-btn {
    display: block;
    width: 100%;
    padding: 6px 12px;
    border: 1px solid var(--vscode-button-border, transparent);
    background: var(--vscode-button-background, #007acc);
    color: var(--vscode-button-foreground, #fff);
    border-radius: 4px;
    cursor: pointer;
    font-size: 12px;
    margin-bottom: 6px;
  }
  .generate-btn:hover {
    background: var(--vscode-button-hoverBackground, #005a9e);
  }
  .last-run {
    font-size: 11px;
    color: var(--vscode-descriptionForeground, #888);
  }
</style>
