<script lang="ts">
  import {
    SvelteFlow,
    Background,
    Controls,
    MiniMap,
    BackgroundVariant,
    type Node,
    type Edge,
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
  import type { IfmlModel } from './types';

  let nodes = $state<Node[]>([]);
  let edges = $state<Edge[]>([]);
  let selectedNodeId = $state<string | null>(null);
  let selectedEdgeId = $state<string | null>(null);
  let currentModel = $state<IfmlModel | null>(null);

  const sync = new SyncClient();
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

  function modelToFlow(model: IfmlModel): { nodes: Node[]; edges: Edge[] } {
    const nodes: Node[] = [];
    const edges: Edge[] = [];

    let y = 50;
    for (const vc of model.viewContainers) {
      const vcId = `vc-${vc.name}`;
      nodes.push({
        id: vcId,
        type: 'view-container',
        position: { x: 50, y },
        data: {
          name: vc.name,
          label: vc.label || vc.name,
          isLandmark: vc.isLandmark,
          isModal: vc.isModal,
          params: vc.params,
          components: vc.components.map(c => ({
            id: `comp-${vc.name}-${c.name}`,
            name: c.name,
            componentType: c.componentType,
            entity: c.entity,
            fields: c.fields,
            filter: c.filter,
            events: c.events,
          })),
        },
      });

      let cy = y + 80;
      for (const comp of vc.components) {
        const compId = `comp-${vc.name}-${comp.name}`;
        nodes.push({
          id: compId,
          type: 'view-component',
          position: { x: 70, y: cy },
          parentId: vcId,
          extent: 'parent' as const,
          data: {
            name: comp.name,
            componentType: comp.componentType,
            entity: comp.entity,
            fields: comp.fields,
            filter: comp.filter,
            events: comp.events,
          },
        });
        cy += 120;
      }

      y += Math.max(vc.components.length * 120 + 100, 160);
    }

    for (const nav of model.navigationEdges) {
      edges.push({
        id: `nav-${nav.sourceContainer}-${nav.targetContainer}`,
        source: `vc-${nav.sourceContainer}`,
        target: `vc-${nav.targetContainer}`,
        type: 'navigation-flow',
        data: {
          label: nav.sourceEvent,
          parameterBinding: nav.parameterBinding,
        },
        markerEnd: { type: 'arrowclosed' },
      });
    }

    return { nodes, edges };
  }

  function onNodeClick(_event: any, node: Node) {
    selectedNodeId = node.id;
    selectedEdgeId = null;
  }

  function onEdgeClick(_event: any, edge: Edge) {
    selectedEdgeId = edge.id;
    selectedNodeId = null;
  }

  let debug = $state('Initializing...');

  // Signal ready to VS Code
  sync.postMessage({ command: 'sync/ready' } as any);

  // Listen for model updates
  sync.onMessage((msg) => {
    if (msg.command === 'sync/modelUpdate') {
      debug = `Model received: ${msg.model.viewContainers.length} views`;
      currentModel = msg.model;
      const flow = modelToFlow(msg.model);
      nodes = flow.nodes;
      edges = flow.edges;
      debug = `Rendered: ${flow.nodes.length} nodes, ${flow.edges.length} edges`;
    }
  });
</script>

<div class="diagram-container">
  <SvelteFlow
    bind:nodes={nodes}
    bind:edges={edges}
    {nodeTypes}
    {edgeTypes}
    fitView
    colorMode="system"
    onnodeclick={onNodeClick}
    onedgeclick={onEdgeClick}
  >
    <Background variant={BackgroundVariant.Dots} />
    <Controls />
    <MiniMap />
  </SvelteFlow>

  <div class="sidebar">
    <div class="debug">{debug}</div>
    <Palette />
    <PropertySheet
      nodeId={selectedNodeId}
      edgeId={selectedEdgeId}
      {nodes}
      {edges}
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
  :global(.svelte-flow) {
    flex: 1;
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
</style>
