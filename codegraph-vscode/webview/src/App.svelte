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
        source: `evt-${nav.sourceContainer}-${nav.sourceEvent}`,
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

  sync.onMessage((msg) => {
    if (msg.command === 'sync/modelUpdate') {
      currentModel = msg.model;
      const flow = modelToFlow(msg.model);
      nodes = flow.nodes;
      edges = flow.edges;
    }
  });

  sync.postMessage({ command: 'sync/diagramChanged', model: { viewContainers: [], actions: [], navigationEdges: [], dataFlows: [], generationOrder: [] } });
</script>

<div class="diagram-container">
  <SvelteFlow
    {nodes}
    {edges}
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
</style>
