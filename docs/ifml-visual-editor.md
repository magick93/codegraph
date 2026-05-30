# IFML Visual Editor — SvelteFlow Diagram Editor

## Overview

A SvelteFlow-based interactive diagram editor that renders IFML models as visual graphs. Embedded in VS Code as a WebView panel. Supports **bidirectional sync** with the text editor: changes in the diagram update the `.ifml` source file and vice versa.

The editor is built with [SvelteFlow](https://svelteflow.dev) (the Svelte-native port of React Flow) from `@xyflow/svelte`, using custom node and edge types that map to IFML concepts.

---

## Architecture

```
┌───────────────────────────────────────────────────────────────────┐
│                    VS Code Extension Host                          │
│                                                                     │
│  ┌─────────────────────────────┐  ┌─────────────────────────────┐  │
│  │    Monaco Text Editor        │  │  WebView Panel              │  │
│  │    (.ifml file)             │  │  (SvelteFlow Diagram)        │  │
│  │                             │  │                              │  │
│  │  ┌───────────────────────┐  │  │  ┌─────────────────────────┐ │  │
│  │  │ Tree-sitter + LSP     │  │  │  │ SvelteFlow Canvas       │ │  │
│  │  │ diagnostics +          │  │  │  │                         │ │  │
│  │  │ completions            │  │  │  │ • ViewContainer nodes   │ │  │
│  │  └───────────────────────┘  │  │  │ • ViewComponent nodes   │ │  │
│  │                             │  │  │ • Event nodes           │ │  │
│  │                             │  │  │ • Action nodes          │ │  │
│  │                             │  │  │ • NavigationFlow edges  │ │  │
│  │                             │  │  │ • DataFlow edges        │ │  │
│  │                             │  │  │ • PropertySheet panel   │ │  │
│  │                             │  │  │ • Palette/toolbox       │ │  │
│  └─────────────┬───────────────┘  │  └─────────────────────────┘ │  │
│                │ sync              │                              │  │
│                │ (LSP ↔ WebSocket) │                              │  │
│                └─────────┬─────────┘                              │  │
│                          │                                         │  │
│                 ┌────────┴────────┐                               │  │
│                 │ Sync Engine     │                               │  │
│                 │ (document model)│                               │  │
│                 └────────┬────────┘                               │  │
└──────────────────────────┼──────────────────────────────────────────┘
                           │ vscode API (postMessage)
                           │ WebSocket (for sync)
┌──────────────────────────┼──────────────────────────────────────────┐
│                  ┌───────┴────────┐                                 │
│                  │  codegraph LSP  │  Rust binary                    │
│                  │  Server         │                                 │
│                  │                 │                                 │
│                  │  • Parse .ifml  │  → diff model                   │
│                  │  • Validate     │  → push to diagram              │
│                  │  • Generate     │                                 │
│                  │    diagram JSON │                                 │
│                  │  • Apply        │                                 │
│                  │    diagram      │                                 │
│                  │    edits to     │                                 │
│                  │    text         │                                 │
│                  └───────┬────────┘                                 │
│                          │                                           │
│                  ┌───────┴────────┐                                 │
│                  │  Grafeo Graph   │                                 │
│                  └────────────────┘                                 │
│                                                                     │
│                        Rust Backend                                 │
└─────────────────────────────────────────────────────────────────────┘
```

---

## SvelteFlow Node Types

Each IFML concept is rendered as a custom SvelteFlow node. Nodes are Svelte components registered with `nodeTypes` on the `<SvelteFlow>` component.

### 1. ViewContainer Node

```
┌───────────────────────────────────┐
│  [🔲] CustomerList     ★ landmark │
│  ┌─────────────────────────────┐  │
│  │  ┌─── ViewComponent ─────┐  │  │
│  │  │ grid : list [Customer]│  │  │
│  │  │ ⚡ select → Detail     │  │  │
│  │  └────────────────────────┘  │  │
│  │  ┌─── ViewComponent ─────┐  │  │
│  │  │ searchBar : form       │  │  │
│  │  │ ⚡ submit → grid       │  │  │
│  │  └────────────────────────┘  │  │
│  └─────────────────────────────┘  │
│  params: { customerId: Uuid }     │
└───────────────────────────────────┘
```

```svelte
<!-- src/webview/ifml-diagram/nodes/ViewContainerNode.svelte -->
<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';
  import type { ViewContainerData } from '../types';

  let { data }: NodeProps<ViewContainerData> = $props();

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
    <!-- Child components shown as embedded cards -->
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
      params: {{ {#each data.params as param}{param.name}: {param.type}{/each} }}
    </div>
  {/if}
</div>

<Handle type="target" position={Position.Top} />
<Handle type="source" position={Position.Bottom} />
```

### 2. ViewComponent Node (standalone, when shown outside container)

```
┌──────────────────────────────┐
│  📋 grid                     │
│  type: list                  │
│  data: Customer              │
│  fields: [name, email, phone]│
│                              │
│  ⚡ select → CustomerDetail  │
│  ⚡ submit → refresh grid    │
└──────────────────────────────┘
```

```svelte
<!-- src/webview/ifml-diagram/nodes/ViewComponentNode.svelte -->
<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';
  import type { ViewComponentData } from '../types';

  let { data }: NodeProps<ViewComponentData> = $props();
</script>

<div class="view-component" class:type-{data.componentType}>
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
        <div class="event-tag" data-event-id={event.id}>
          ⚡ {event.type}
        </div>
      {/each}
    </div>
  {/if}
</div>

<Handle type="target" position={Position.Top} />
<Handle type="source" position={Position.Bottom} />
```

### 3. Event Node (detached view)

```
┌──────────────────┐
│  ⚡ select       │
│  (row)           │
│  params: row.id  │
└──────┬──┬────────┘
       │  │
       ▼  └──→ [navigate]
```

```svelte
<!-- src/webview/ifml-diagram/nodes/EventNode.svelte -->
<script lang="ts">
  import { Handle, Position, type NodeProps } from '@xyflow/svelte';
  import type { EventData } from '../types';

  let { data }: NodeProps<EventData> = $props();

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

<div class="event-node" class:event-{data.eventType}>
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
```

### 4. Action Node

```
┌──────────────────────┐
│  🛠️ UpdateCustomer   │
│  on success → List   │
│  on error   → Detail │
└──────┬───────────────┘
       │
       ▼
```

```svelte
<!-- src/webview/ifml-diagram/nodes/ActionNode.svelte -->
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
```

---

## SvelteFlow Edge Types

### 1. Navigation Flow

```
──────► [CustomerDetail]
  select
  {customerId: row.id}
```

```svelte
<!-- src/webview/ifml-diagram/edges/NavigationFlowEdge.svelte -->
<script lang="ts">
  import { BaseEdge, getSmoothStepPath, type EdgeProps } from '@xyflow/svelte';
  import type { NavigationFlowData } from '../types';

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
```

### 2. Data Flow

```
- - - - ► [component]
  passes: searchTerm
```

Uses dashed line to distinguish from navigation:

```svelte
<!-- src/webview/ifml-diagram/edges/DataFlowEdge.svelte -->
<script lang="ts">
  import { BaseEdge, getBezierPath, type EdgeProps } from '@xyflow/svelte';

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
```

### 3. Parameter Binding (sub-edge on navigation/data flow)

Shown as a small annotation label on the flow edge, displaying the parameter map.

---

## Main SvelteFlow Component

```svelte
<!-- src/webview/ifml-diagram/IfmlDiagram.svelte -->
<script lang="ts">
  import SvelteFlow, {
    Background,
    Controls,
    MiniMap,
    Panel,
    BackgroundVariant,
    useNodes,
    useEdges,
    addEdge,
    type Connection,
  } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import ViewContainerNode from './nodes/ViewContainerNode.svelte';
  import ViewComponentNode from './nodes/ViewComponentNode.svelte';
  import EventNode from './nodes/EventNode.svelte';
  import ActionNode from './nodes/ActionNode.svelte';
  import NavigationFlowEdge from './edges/NavigationFlowEdge.svelte';
  import DataFlowEdge from './edges/DataFlowEdge.svelte';
  import Palette from './Palette.svelte';
  import PropertySheet from './PropertySheet.svelte';
  import type { IfmlModel, Node, Edge } from './types';
  import { syncEngine } from './sync';

  // Props
  let { model, onModelChange } = $props<{
    model: IfmlModel;
    onModelChange: (m: IfmlModel) => void;
  }>();

  let nodes = $state<Node[]>([]);
  let edges = $state<Edge[]>([]);
  let selectedElement = $state<string | null>(null);

  const nodeTypes = {
    'view-container': ViewContainerNode,
    'view-component': ViewComponentNode,
    'event': EventNode,
    'action': ActionNode,
  };

  const edgeTypes = {
    'navigation-flow': NavigationFlowEdge,
    'data-flow': DataFlowEdge,
  };

  // Convert IFML model to SvelteFlow nodes/edges
  function modelToFlow(m: IfmlModel): { nodes: Node[]; edges: Edge[] } {
    const nodes: Node[] = [];
    const edges: Edge[] = [];

    // Layout engine: ELK or Dagre (hierarchical top-to-bottom)
    // For now, use a simple grid layout with parent/child grouping
    let x = 50, y = 50;

    for (const vc of m.viewContainers) {
      const vcId = `vc-${vc.name}`;
      nodes.push({
        id: vcId,
        type: 'view-container',
        position: { x, y },
        data: vc,
      });

      // Components inside
      let cy = y + 80;
      for (const comp of vc.components) {
        const compId = `comp-${vc.name}-${comp.name}`;
        nodes.push({
          id: compId,
          type: 'view-component',
          position: { x: x + 20, cy },
          parentId: vcId,
          extent: 'parent',
          data: comp,
        });

        // Events inside components
        for (const event of comp.events) {
          const eventId = `evt-${vc.name}-${comp.name}-${event.type}`;
          nodes.push({
            id: eventId,
            type: 'event',
            position: { x: x + 40, y: cy + 60 },
            parentId: vcId,
            data: event,
          });

          // Navigation/data flow edges from events
          if (event.action.type === 'navigate') {
            edges.push({
              id: `flow-${eventId}-to-${event.action.target}`,
              source: eventId,
              target: `vc-${event.action.target}`,
              type: 'navigation-flow',
              data: {
                label: event.type,
                parameterBinding: event.action.binding?.pairs || {},
              },
              markerEnd: { type: MarkerType.ArrowClosed },
            });
          }
        }
        cy += 160;
      }
      y += (vc.components.length * 160) + 100;
    }

    return { nodes, edges };
  }

  // Convert SvelteFlow nodes/edges back to IFML model
  function flowToModel(ns: Node[], es: Edge[]): IfmlModel {
    // ... reconstruct IFML model from node data attributes
  }

  // Initialize
  let flow = $derived(modelToFlow(model));
  nodes = flow.nodes;
  edges = flow.edges;

  function onConnect(connection: Connection) {
    edges = addEdge(connection, edges);
    // Notify sync engine
    syncEngine.sendDiagramChange(flowToModel(nodes, edges));
  }

  function onNodesChange(changes: any) {
    nodes = applyNodeChanges(changes, nodes);
  }

  function onEdgesChange(changes: any) {
    edges = applyEdgeChanges(changes, edges);
  }

  // Listen for text editor changes
  onMount(() => {
    syncEngine.onModelFromText((m: IfmlModel) => {
      const { nodes: newNodes, edges: newEdges } = modelToFlow(m);
      nodes = newNodes;
      edges = newEdges;
    });
  });
</script>

<div class="diagram-container">
  <SvelteFlow
    {nodes}
    {edges}
    {nodeTypes}
    {edgeTypes}
    fitView
    colorMode="system"
    onconnect={onConnect}
    onnodeschange={onNodesChange}
    onedgeschange={onEdgesChange}
    onnodeclick={(_, node) => selectedElement = node.id}
  >
    <Background variant={BackgroundVariant.Dots} />
    <Controls />
    <MiniMap />
    <Panel position="top-left">
      <Palette />
    </Panel>
  </SvelteFlow>

  <PropertySheet
    elementId={selectedElement}
    {nodes}
    {edges}
    onUpdate={(id, data) => {
      // Update node/edge data and sync
      syncEngine.sendDiagramChange(flowToModel(nodes, edges));
    }}
  />
</div>
```

---

## Palette / Toolbox

Drag-and-drop palette for adding new elements:

```svelte
<!-- src/webview/ifml-diagram/Palette.svelte -->
<script lang="ts">
  let searchQuery = $state('');
  const items = [
    { type: 'view-container', label: 'View Container', icon: '🔲' },
    { type: 'view-component', label: 'List', icon: '📋', componentType: 'list' },
    { type: 'view-component', label: 'Form', icon: '📝', componentType: 'form' },
    { type: 'view-component', label: 'Details', icon: '📄', componentType: 'details' },
    { type: 'view-component', label: 'Search', icon: '🔍', componentType: 'search' },
    { type: 'view-component', label: 'Tree', icon: '🌳', componentType: 'tree' },
    { type: 'event', label: 'Event', icon: '⚡' },
    { type: 'action', label: 'Action', icon: '🛠️' },
  ];

  let filtered = $derived(
    searchQuery
      ? items.filter(i => i.label.toLowerCase().includes(searchQuery.toLowerCase()))
      : items
  );

  function onDragStart(e: DragEvent, item: typeof items[0]) {
    e.dataTransfer?.setData('application/ifml-node', JSON.stringify(item));
    e.dataTransfer!.effectAllowed = 'move';
  }
</script>

<div class="palette">
  <input type="text" placeholder="Search elements..." bind:value={searchQuery} />
  <div class="items">
    {#each filtered as item}
      <div
        class="palette-item"
        draggable="true"
        ondragstart={(e) => onDragStart(e, item)}
      >
        <span class="icon">{item.icon}</span>
        <span class="label">{item.label}</span>
      </div>
    {/each}
  </div>
</div>
```

---

## Property Sheet

Side panel showing properties of the selected element:

```svelte
<!-- src/webview/ifml-diagram/PropertySheet.svelte -->
<script lang="ts">
  import type { Node, Edge } from '@xyflow/svelte';

  let { elementId, nodes, edges, onUpdate } = $props<{
    elementId: string | null;
    nodes: Node[];
    edges: Edge[];
    onUpdate: (id: string, data: any) => void;
  }>();

  let selectedNode = $derived(nodes.find(n => n.id === elementId));
  let selectedEdge = $derived(edges.find(e => e.id === elementId));
  let element = $derived(selectedNode || selectedEdge);

  let editBuffer = $state<Record<string, any>>({});

  $effect(() => {
    if (element) {
      editBuffer = { ...element.data };
    }
  });

  function saveChanges() {
    if (elementId) {
      onUpdate(elementId, editBuffer);
    }
  }
</script>

<aside class="property-sheet">
  {#if element}
    <h3>{element.data?.name || element.id}</h3>
    <p class="type-label">{element.type || element.data?.componentType}</p>

    <div class="field-group">
      {#each Object.entries(editBuffer) as [key, value]}
        {#if typeof value === 'string'}
          <label>
            <span>{key}</span>
            <input type="text" bind:value={editBuffer[key]} onchange={saveChanges} />
          </label>
        {:else if typeof value === 'boolean'}
          <label class="checkbox">
            <input type="checkbox" checked={value} onchange={(e) => {
              editBuffer[key] = e.target.checked;
              saveChanges();
            }} />
            <span>{key}</span>
          </label>
        {:else if Array.isArray(value)}
          <label>
            <span>{key}</span>
            <input
              type="text"
              value={value.join(', ')}
              onchange={(e) => {
                editBuffer[key] = e.target.value.split(',').map((s: string) => s.trim());
                saveChanges();
              }}
            />
          </label>
        {/if}
      {/each}
    </div>

    {#if selectedNode?.type === 'view-component'}
      <div class="schema-preview">
        <h4>Entity: {element.data?.entity}</h4>
        <p class="hint">Fields from JSON Schema</p>
        <!-- Injected by sync engine: list of entity fields with types -->
        <div class="field-list">
          {#each element.data?._entityFields || [] as field}
            <div class="field-item">
              <span class="f-name">{field.name}</span>
              <span class="f-type">{field.type}</span>
              <span class="f-required">{field.required ? '*' : ''}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {:else}
    <p class="empty">Select an element to edit</p>
  {/if}
</aside>
```

---

## Sync Engine

The bidirectional sync engine is the critical piece. It maintains the invariant that the text representation is always the source of truth.

### Sync States

```
        ┌────────────────────────────────────────────┐
        │            SyncEngine State                 │
        │                                            │
        │  idle ── editing-text ── debounce ── sync  │
        │    │                     ▲                  │
        │    └── editing-diagram ──┘                  │
        │                                            │
        │  (prevents feedback loops via               │
        │   edit-source tracking)                     │
        └────────────────────────────────────────────┘
```

```typescript
// src/webview/sync.ts
import { VsCodeBridge } from './vscode-bridge';

export interface IfmlModel {
  viewContainers: ViewContainer[];
  actions: Action[];
  modules: Module[];
}

export class SyncEngine {
  private lastEditSource: 'text' | 'diagram' | null = null;
  private debounceTimer: ReturnType<typeof setTimeout> | null = null;
  private pendingModel: IfmlModel | null = null;

  constructor(private bridge: VsCodeBridge) {
    // Listen for model updates from the text editor (via LSP → extension host → webview)
    bridge.onMessage('sync/modelUpdate', (model: IfmlModel) => {
      if (this.lastEditSource === 'diagram') return; // Prevent feedback loop
      this.lastEditSource = 'text';
      this.onModelFromText?.(model);
      setTimeout(() => { this.lastEditSource = null; }, 100);
    });

    // Listen for selection sync
    bridge.onMessage('sync/selectElement', (elementId: string) => {
      this.onSelectElement?.(elementId);
    });
  }

  // Called when the diagram is changed via user interaction
  sendDiagramChange(model: IfmlModel): void {
    if (this.lastEditSource === 'text') return; // Prevent feedback loop

    this.lastEditSource = 'diagram';
    this.pendingModel = model;

    // Debounce: batch rapid diagram changes
    if (this.debounceTimer) clearTimeout(this.debounceTimer);
    this.debounceTimer = setTimeout(() => {
      this.bridge.postMessage('sync/textEdit', {
        // Serialize model back to DSL text
        text: this.modelToText(model),
      });
      this.lastEditSource = null;
    }, 300);
  }

  // Called when user clicks an element in the diagram
  sendElementSelection(elementId: string): void {
    this.bridge.postMessage('sync/selectInText', { elementId });
  }

  // Callbacks for the SvelteFlow component
  onModelFromText: ((model: IfmlModel) => void) | null = null;
  onSelectElement: ((elementId: string) => void) | null = null;

  private modelToText(model: IfmlModel): string {
    // Generate DSL text from the model graph
    // This is the inverse of parse — it serializes the in-memory graph
    // back to the concrete IFML DSL syntax
    const lines: string[] = [];

    for (const vc of model.viewContainers) {
      lines.push(`view "${vc.name}" {`);
      if (vc.params?.length) {
        lines.push(`    params { ${vc.params.map(p => `${p.name}: ${p.type}`).join(', ')} };`);
      }
      for (const comp of vc.components) {
        lines.push(`    component "${comp.name}" {`);
        lines.push(`        type: ${comp.componentType};`);
        if (comp.entity) lines.push(`        data: ${comp.entity};`);
        if (comp.fields?.length) lines.push(`        fields: [${comp.fields.join(', ')}];`);
        if (comp.filter) lines.push(`        filter: ${comp.filter};`);
        for (const event of comp.events || []) {
          const params = event.params?.length ? `(${event.params.join(', ')})` : '';
          const binding = event.action.binding
            ? `, { ${Object.entries(event.action.binding.pairs).map(([k, v]) => `${k}: ${v}`).join(', ')} }`
            : '';
          lines.push(`        on ${event.type}${params} -> ${event.action.type}("${event.action.target}"${binding});`);
        }
        lines.push(`    }`);
      }
      lines.push(`}`);
      lines.push(``);
    }

    return lines.join('\n');
  }
}
```

---

## Model → Text Serialization

The sync engine must be able to serialize the diagram model back to valid DSL text. This is the **model-to-text** pass:

1. Walk all view containers in the model
2. For each, emit `view "Name" { ... }` block
3. For each component, emit `component "Name" { ... }` block
4. For each event, emit `on type(params) -> action("target", { binding })`
5. Preserve all property assignments

This is the inverse of the Pest parser. The serialized text is sent to the LSP server which applies it as a text edit (replacing the entire document contents).

On the LSP side, the text edit is applied, then the file is re-parsed and new diagnostics are sent. This ensures validation consistency.

---

## Layout Engine

Use **ELK** (Eclipse Layout Kernel) via the `sprotty-elk` integration or the `@xyflow/svelte` built-in layout support. The layout is:

- **Hierarchical** (top-to-bottom) for navigation flows
- **Containers** are rendered as groups (parent nodes with `extent: 'parent'`)
- Events are small nodes aligned to the right of their parent component
- Actions are small nodes aligned below the event that triggers them

```typescript
// src/webview/ifml-diagram/layout.ts
import ELK from 'elkjs/lib/elk-api';

const elk = new ELK();

export async function layoutModel(nodes: Node[], edges: Edge[]): Promise<{ nodes: Node[]; edges: Edge[] }> {
  const graph = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'DOWN',
      'elk.spacing.nodeNode': '40',
      'elk.layered.spacing.edgeNodeBetweenLayers': '30',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX',
    },
    children: nodes.map(n => ({
      id: n.id,
      width: n.width || 280,
      height: n.height || 120,
      parentId: n.parentId,
    })),
    edges: edges.map(e => ({
      id: e.id,
      sources: [e.source],
      targets: [e.target],
    })),
  };

  const layout = await elk.layout(graph);

  const laidOutNodes = nodes.map(n => {
    const ln = layout.children?.find(c => c.id === n.id);
    return ln ? { ...n, position: { x: ln.x!, y: ln.y! } } : n;
  });

  return { nodes: laidOutNodes, edges };
}
```

---

## VS Code WebView Integration

The SvelteFlow app is served inside a VS Code WebView panel:

```typescript
// src/webview/panel.ts
import * as vscode from 'vscode';
import { getNonce } from './util';

export function openDiagramPanel(context: vscode.ExtensionContext, uri: vscode.Uri) {
    const panel = vscode.window.createWebviewPanel(
        'ifmlDiagram',
        'IFML Diagram',
        vscode.ViewColumn.Beside,
        {
            enableScripts: true,
            localResourceRoots: [
                vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview'),
            ],
            retainContextWhenHidden: true,
        }
    );

    const scriptUri = panel.webview.asWebviewUri(
        vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview', 'ifml-diagram.js')
    );
    const styleUri = panel.webview.asWebviewUri(
        vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview', 'ifml-diagram.css')
    );

    panel.webview.html = `
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1.0" />
            <meta http-equiv="Content-Security-Policy" content="
                default-src 'none';
                style-src ${panel.webview.cspSource} 'unsafe-inline';
                script-src 'nonce-${getNonce()}' 'wasm-unsafe-eval';
                font-src ${panel.webview.cspSource};
            " />
            <link rel="stylesheet" href="${styleUri}" />
        </head>
        <body>
            <div id="root"></div>
            <script nonce="${getNonce()}" src="${scriptUri}"></script>
        </body>
        </html>
    `;

    // Bridge: VS Code ↔ WebView
    const bridge = new VsCodeBridge(panel);
    const syncEngine = new SyncEngine(bridge);

    // Listen for document changes from the text editor
    const watcher = vscode.workspace.onDidChangeTextDocument((e) => {
        if (e.document.uri.toString() === uri.toString()) {
            // Ask LSP server to parse and send model update
            vscode.commands.executeCommand(
                'lsp.sendNotification',
                'sync/documentChanged',
                { uri: uri.toString(), version: e.document.version }
            );
        }
    });

    panel.onDidDispose(() => watcher.dispose());
}
```

---

## Svelte + Vite Build Pipeline

The WebView is built as a separate Svelte app using Vite:

```typescript
// vite.config.ts (in src/webview/ifml-diagram/)
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  build: {
    outDir: '../../../dist/webview',
    lib: {
      entry: './main.ts',
      formats: ['iife'],
      name: 'IfmlDiagram',
    },
    rollupOptions: {
      output: {
        entryFileNames: 'ifml-diagram.js',
        assetFileNames: 'ifml-diagram.css',
      },
    },
  },
});
```

```typescript
// src/webview/ifml-diagram/main.ts
import App from './IfmlDiagram.svelte';
import { mount } from 'svelte';

const app = mount(App, {
  target: document.getElementById('root')!,
});

export default app;
```

---

## Codelist + JSON Schema Side Panel

A secondary panel showing loaded JSON Schema entities for drag-to-bind:

```
┌─────────────────────────────┐
│  📦 Schema Browser          │
│                             │
│  ▶ Sales                    │
│    ├─ Customer              │
│    │  ├─ name: String      │
│    │  ├─ email: String     │
│    │  ├─ phone: String     │
│    │  ├─ status: Codelist  │
│    │  └─ orders: Order[]   │
│    ├─ Order                 │
│    └─ Product               │
│                             │
│  ▼ Inventory                │
│    ├─ Warehouse             │
│    └─ StockItem             │
└─────────────────────────────┘
```

Entities can be dragged from this panel onto a ViewComponent node in the diagram to set the `data:` binding. Fields can be dragged to add to the `fields:` array.

---

## Validation Overlay

Validation errors from the LSP server are shown as overlays on diagram elements:

```
┌────────────────────┐
│  grid : list       │
│  🔴 Customer2      │  ← red border, tooltip: "Entity not found"
│                    │
│  fields: [name,    │
│    🔴 emial]       │  ← red underline on "emial", tooltip: "Field not found"
└────────────────────┘
```

The sync engine receives diagnostics from the LSP server and maps them to diagram elements by matching source ranges to node/edge IDs.

---

## Testing Strategy

| Scope | Test | Method |
|---|---|---|
| Node rendering | Each node type renders with correct data | Vitest + jsdom |
| Edge rendering | Navigation/Data flow edges render with correct markers | Vitest + jsdom |
| Layout | Given a model, layout positions are deterministic | Vitest + ELK snapshot |
| Model → Flow | `modelToFlow()` produces correct nodes/edges | Vitest |
| Flow → Model | `flowToModel()` round-trips correctly | Vitest |
| Sync engine | `sendDiagramChange()` produces correct text diffs | Vitest |
| Text → Diagram | LSP model update triggers correct diagram re-render | Integration |
| Drag-and-drop | Dragging palette item creates new node | Playwright |
| Property editing | Changing property emits correct update | Playwright |
| VS Code integration | WebView loads, receives messages | VS Code extension tests |
