<script lang="ts">
  import {
    SvelteFlow,
    Background,
    Controls,
    MiniMap,
    Panel,
    addEdge,
    useSvelteFlow,
    BackgroundVariant,
    type Node,
    type Edge,
    type Connection,
    type NodeTargetEventWithPointer,
  } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import ViewContainerNode from './nodes/ViewContainerNode.svelte';
  import ViewComponentNode from './nodes/ViewComponentNode.svelte';
  import EventNode from './nodes/EventNode.svelte';
  import ActionNode from './nodes/ActionNode.svelte';
  import NavigationFlowEdge from './edges/NavigationFlowEdge.svelte';
  import DataFlowEdge from './edges/DataFlowEdge.svelte';
  import Palette from './palette/Palette.svelte';
  import PropertySheet from './property-sheet/PropertySheet.svelte';
  import { SyncClient } from './sync';
  import type { IfmlModel, CodegenConfig } from './types';
  import { modelToFlow, flowToModel, positionsEqual, computeLayeredLayout } from './layout';

  let nodes = $state<Node[]>([]);
  let edges = $state<Edge[]>([]);
  let selectedNodeId = $state<string | null>(null);
  let selectedEdgeId = $state<string | null>(null);
  let currentModel = $state<IfmlModel | null>(null);
  let codegenConfig = $state<CodegenConfig | null>(null);
  let dragSyncTimer: ReturnType<typeof setTimeout> | null = null;
  let dropCount = 0;

  const sync = new SyncClient();
  const flow = useSvelteFlow();
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

  function onNodeClick(_event: any, node: Node | undefined) {
    selectedNodeId = node?.id ?? null;
    selectedEdgeId = null;
  }

  function onEdgeClick(_event: any, edge: Edge | undefined) {
    selectedEdgeId = edge?.id ?? null;
    selectedNodeId = null;
  }

  function onNodeDragStop(_event: NodeTargetEventWithPointer) {
    if (dragSyncTimer) clearTimeout(dragSyncTimer);
    dragSyncTimer = setTimeout(() => {
      if (!currentModel) return;
      const updated = flowToModel(nodes, currentModel);
      if (!hasMoved(updated, currentModel)) return;
      currentModel = updated;
      sync.sendDiagramChange(updated);
    }, 300);
  }

  function hasMoved(a: IfmlModel, b: IfmlModel): boolean {
    for (let i = 0; i < a.viewContainers.length; i++) {
      const va = a.viewContainers[i];
      const vb = b.viewContainers[i];
      if (!va || !vb) continue;
      if (!positionsEqual(va.position ?? { x: 0, y: 0 }, vb.position ?? { x: 0, y: 0 })) return true;
      for (let j = 0; j < va.components.length; j++) {
        const ca = va.components[j];
        const cb = vb.components[j];
        if (!ca || !cb) continue;
        if (!positionsEqual(ca.position ?? { x: 0, y: 0 }, cb.position ?? { x: 0, y: 0 })) return true;
      }
    }
    return false;
  }

  function applyModel(model: IfmlModel): void {
    const prev = new Map(nodes.map(n => [n.id, n.position] as const));
    currentModel = model;
    const flowResult = modelToFlow(model, prev);
    nodes = flowResult.nodes;
    edges = flowResult.edges;
  }

  function onDrop(event: DragEvent) {
    event.preventDefault();
    if (!event.dataTransfer || !currentModel) return;
    const raw = event.dataTransfer.getData('application/ifml-node');
    if (!raw) return;
    let item: { type: string; componentType?: string };
    try {
      item = JSON.parse(raw);
    } catch {
      return;
    }

    const area = (event.currentTarget as HTMLElement)?.getBoundingClientRect();
    const viewport = flow.getViewport();
    const x = area ? (event.clientX - area.left - viewport.x) / viewport.zoom : 100;
    const y = area ? (event.clientY - area.top - viewport.y) / viewport.zoom : 100;

    if (item.type === 'view-container') {
      dropCount += 1;
      const name = `NewView${dropCount}`;
      const updated: IfmlModel = {
        ...currentModel,
        viewContainers: [
          ...currentModel.viewContainers,
          {
            name,
            label: name,
            isXor: false,
            isDefault: false,
            isLandmark: false,
            isModal: false,
            params: [],
            components: [],
            events: [],
            containers: [],
            position: { x, y },
          },
        ],
        generationOrder: [...currentModel.generationOrder, name],
      };
      applyModel(updated);
      sync.sendDiagramChange(updated);
      return;
    }

    if (item.type === 'view-component' && item.componentType) {
      const containers = currentModel.viewContainers;
      if (containers.length === 0) return;
      const targetIdx = Math.max(
        0,
        containers.findIndex(vc => `vc-${vc.name}` === selectedNodeId),
      );
      const target = containers[targetIdx];
      const compName = `${item.componentType}${dropCount === 0 ? '' : dropCount}`;
      dropCount += 1;
      const updated: IfmlModel = {
        ...currentModel,
        viewContainers: currentModel.viewContainers.map((vc, idx) => {
          if (idx !== targetIdx) return vc;
          return {
            ...vc,
            components: [
              ...vc.components,
              {
                name: compName,
                componentType: item.componentType!,
                entity: undefined,
                fields: [],
                filter: undefined,
                properties: {},
                events: [],
                parts: [],
                position: { x: x + 20, y },
              },
            ],
          };
        }),
      };
      applyModel(updated);
      sync.sendDiagramChange(updated);
    }
  }

  function onConnect(connection: Connection) {
    if (!currentModel) return;
    const sourceVc = nodes.find(n => n.id === connection.source);
    const targetVc = nodes.find(n => n.id === connection.target);
    if (!sourceVc || !targetVc) return;
    const sourceName = sourceVc.data?.name as string;
    const targetName = targetVc.data?.name as string;
    if (!sourceName || !targetName) return;
    if (currentModel.navigationEdges.some(e => e.sourceContainer === sourceName && e.targetContainer === targetName)) {
      return;
    }
    const edge = addEdge(
      {
        id: `nav-${sourceName}-${targetName}`,
        source: `vc-${sourceName}`,
        target: `vc-${targetName}`,
        type: 'navigation-flow',
        data: { label: 'select', parameterBinding: undefined },
        markerEnd: { type: 'arrowclosed' },
      },
      edges,
    );
    edges = edge;
    const updated: IfmlModel = {
      ...currentModel,
      navigationEdges: [
        ...currentModel.navigationEdges,
        { sourceContainer: sourceName, sourceEvent: 'select', targetContainer: targetName },
      ],
    };
    currentModel = updated;
    sync.sendDiagramChange(updated);
  }

  function handleNodeUpdate(nodeId: string, patch: Record<string, unknown>) {
    const node = nodes.find(n => n.id === nodeId);
    if (!node || !currentModel) return;
    node.data = { ...node.data, ...patch };
    const updated: IfmlModel = {
      ...currentModel,
      viewContainers: currentModel.viewContainers.map(vc => {
        if (nodeId === `vc-${vc.name}`) return { ...vc, ...patch } as IfmlModel['viewContainers'][number];
        return {
          ...vc,
          components: vc.components.map(c =>
            nodeId === `comp-${vc.name}-${c.name}` ? { ...c, ...patch } : c,
          ),
        };
      }),
    };
    currentModel = updated;
    sync.sendDiagramChange(updated);
  }

  function onAutoLayout() {
    if (!currentModel) return;
    const layout = computeLayeredLayout(currentModel);
    for (const n of nodes) {
      const pos = layout.get(n.id);
      if (pos) n.position = { x: pos.x, y: pos.y };
    }
    const updated = flowToModel(nodes, currentModel);
    currentModel = updated;
    sync.sendDiagramChange(updated);
  }

  let debug = $state('Initializing...');

  // Signal ready to VS Code
  sync.postMessage({ command: 'sync/ready' } as any);

  // Listen for model updates
  sync.onMessage((msg) => {
    if (msg.command === 'sync/modelUpdate') {
      debug = `Model received: ${msg.model.viewContainers.length} views`;
      const prev = new Map(nodes.map(n => [n.id, n.position] as const));
      currentModel = msg.model;
      const flowResult = modelToFlow(msg.model, prev);
      nodes = flowResult.nodes;
      edges = flowResult.edges;
      debug = `Rendered: ${flowResult.nodes.length} nodes, ${flowResult.edges.length} edges`;
    }
    if (msg.command === 'sync/codegenConfig') {
      codegenConfig = msg.config;
    }
  });
</script>

<div class="diagram-container">
  <div
    class="flow-area"
    role="application"
    ondragover={(e) => e.preventDefault()}
    ondrop={onDrop}
  >
    <SvelteFlow
      bind:nodes={nodes}
      bind:edges={edges}
      {nodeTypes}
      {edgeTypes}
      fitView
      colorMode="system"
      onnodeclick={onNodeClick}
      onedgeclick={onEdgeClick}
      onnodedragstop={onNodeDragStop}
      onconnect={onConnect}
    >
      <Background variant={BackgroundVariant.Dots} />
      <Controls />
      <MiniMap />
      <Panel position="top-right">
        <button class="toolbar-btn" onclick={onAutoLayout}>↕ Auto Layout</button>
      </Panel>
    </SvelteFlow>
  </div>

  <div class="sidebar">
    <div class="debug">{debug}</div>
    <Palette />
    <PropertySheet
      nodeId={selectedNodeId}
      edgeId={selectedEdgeId}
      {nodes}
      {edges}
      {codegenConfig}
      onUpdate={handleNodeUpdate}
    />
  </div>
</div>

<style>
  :global(body) { margin: 0; padding: 0; }
  .diagram-container {
    width: 100%;
    height: 100vh;
    display: flex;
  }
  .flow-area {
    flex: 1;
    position: relative;
    min-width: 0;
  }
  :global(.svelte-flow) {
    width: 100%;
    height: 100%;
  }
  .sidebar {
    width: 280px;
    border-left: 1px solid var(--vscode-panel-border, #ccc);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .debug {
    padding: 8px;
    font-size: 11px;
    color: var(--vscode-editorInfo-foreground, #888);
    font-family: monospace;
    background: var(--vscode-editor-background, #1e1e1e);
    border-bottom: 1px solid var(--vscode-panel-border, #ccc);
  }
  .toolbar-btn {
    padding: 4px 10px;
    border: 1px solid var(--vscode-panel-border, #ccc);
    border-radius: 4px;
    background: var(--vscode-button-secondaryBackground, #2d2d30);
    color: var(--vscode-button-secondaryForeground, #ddd);
    font-size: 12px;
    cursor: pointer;
  }
  .toolbar-btn:hover {
    background: var(--vscode-button-secondaryHoverBackground, #3e3e42);
  }
</style>
