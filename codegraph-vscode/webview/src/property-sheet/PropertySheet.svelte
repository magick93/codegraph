<script lang="ts">
  import type { Node, Edge } from '@xyflow/svelte';
  import type { CodegenConfig } from '../types';
  import { SyncClient } from '../sync';

  let { nodeId, edgeId, nodes, edges, codegenConfig, onUpdate }: {
    nodeId: string | null;
    edgeId: string | null;
    nodes: Node[];
    edges: Edge[];
    codegenConfig: CodegenConfig | null;
    onUpdate?: (nodeId: string, patch: Record<string, unknown>) => void;
  } = $props();

  let selectedNode = $derived(nodes.find(n => n.id === nodeId));
  let selectedEdge = $derived(edges.find(e => e.id === edgeId));
  let element = $derived(selectedNode || selectedEdge);

  const sync = new SyncClient();

  // ── Editable fields ────────────────────────────────────────────
  interface EditableField {
    key: string;
    label: string;
    kind: 'string' | 'bool' | 'array';
  }

  let editableFields = $derived.by<EditableField[]>(() => {
    if (!element || !element.data) return [];
    if (element.type === 'view-container') {
      return [
        { key: 'label', label: 'Label', kind: 'string' },
        { key: 'isLandmark', label: 'Landmark', kind: 'bool' },
        { key: 'isModal', label: 'Modal', kind: 'bool' },
      ];
    }
    if (element.type === 'view-component') {
      return [
        { key: 'componentType', label: 'Type', kind: 'string' },
        { key: 'entity', label: 'Entity', kind: 'string' },
        { key: 'mode', label: 'Mode', kind: 'string' },
        { key: 'fields', label: 'Fields', kind: 'array' },
        { key: 'filter', label: 'Filter', kind: 'string' },
      ];
    }
    return [];
  });

  let draft = $state<Record<string, string | boolean>>({});
  let draftKey = $state<string>('');

  $effect(() => {
    const id = element?.id;
    if (!id) {
      draft = {};
      draftKey = '';
      return;
    }
    if (id !== draftKey) {
      const next: Record<string, string | boolean> = {};
      for (const f of editableFields) {
        const raw = (element.data as Record<string, unknown>)[f.key];
        if (f.kind === 'bool') next[f.key] = Boolean(raw);
        else if (f.kind === 'array') next[f.key] = Array.isArray(raw) ? (raw as string[]).join(', ') : '';
        else next[f.key] = typeof raw === 'string' ? raw : '';
      }
      draft = next;
      draftKey = id;
    }
  });

  let savedFlash = $state(false);

  function save() {
    if (!element || !onUpdate) return;
    const patch: Record<string, unknown> = {};
    for (const f of editableFields) {
      const value = draft[f.key];
      if (f.kind === 'bool') patch[f.key] = Boolean(value);
      else if (f.kind === 'array') {
        patch[f.key] = String(value)
          .split(',')
          .map(s => s.trim())
          .filter(Boolean);
      } else {
        patch[f.key] = String(value);
      }
    }
    onUpdate(element.id, patch);
    savedFlash = true;
    setTimeout(() => { savedFlash = false; }, 1200);
  }

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
    {#if editableFields.length > 0}
      <div class="edit-section">
        {#each editableFields as f}
          {#if f.kind === 'bool'}
            <label class="field checkbox">
              <input
                type="checkbox"
                checked={Boolean(draft[f.key])}
                onchange={(e) => { draft = { ...draft, [f.key]: e.currentTarget.checked }; }}
              />
              <span class="label">{f.label}</span>
            </label>
          {:else if f.kind === 'array'}
            <div class="field">
              <span class="label">{f.label} (comma-separated)</span>
              <input
                class="text-input"
                type="text"
                value={String(draft[f.key] ?? '')}
                oninput={(e) => { draft = { ...draft, [f.key]: e.currentTarget.value }; }}
              />
            </div>
          {:else}
            <div class="field">
              <span class="label">{f.label}</span>
              <input
                class="text-input"
                type="text"
                value={String(draft[f.key] ?? '')}
                oninput={(e) => { draft = { ...draft, [f.key]: e.currentTarget.value }; }}
              />
            </div>
          {/if}
        {/each}
        <div class="save-row">
          <button class="save-btn" onclick={save}>💾 Save</button>
          {#if savedFlash}
            <span class="saved-flash">Saved</span>
          {/if}
        </div>
      </div>
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
  .field.checkbox {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .field.checkbox .label {
    margin-bottom: 0;
    font-size: 12px;
  }
  .text-input {
    width: 100%;
    box-sizing: border-box;
    padding: 4px 6px;
    border: 1px solid var(--vscode-input-border, #444);
    border-radius: 4px;
    background: var(--vscode-input-background, #1e1e1e);
    color: var(--vscode-input-foreground, #ddd);
    font-size: 12px;
  }
  .text-input:focus {
    outline: 1px solid var(--vscode-focusBorder, #007acc);
  }
  .edit-section {
    border-top: 1px solid var(--vscode-panel-border, #333);
    padding-top: 10px;
    margin-top: 10px;
  }
  .save-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }
  .save-btn {
    padding: 5px 12px;
    border: 1px solid var(--vscode-button-border, transparent);
    background: var(--vscode-button-background, #007acc);
    color: var(--vscode-button-foreground, #fff);
    border-radius: 4px;
    cursor: pointer;
    font-size: 12px;
  }
  .save-btn:hover {
    background: var(--vscode-button-hoverBackground, #005a9e);
  }
  .saved-flash {
    font-size: 11px;
    color: var(--vscode-testing-iconPassed, #89d185);
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
