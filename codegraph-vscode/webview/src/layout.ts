import type { Node, Edge } from '@xyflow/svelte';
import type { IfmlModel, ViewContainerData, ViewComponentData } from './types';

export interface XY {
  x: number;
  y: number;
}

// Deterministic grid fallback positions — the same math both directions use
// so flowToModel can skip nodes that sit exactly on the fallback.
export function gridPositions(model: IfmlModel): Map<string, XY> {
  const map = new Map<string, XY>();
  let y = 50;
  for (const vc of model.viewContainers) {
    const vcFallback = { x: 50, y };
    const vcResolved = vc.position ?? vcFallback;
    map.set(`vc-${vc.name}`, vcFallback);
    let cy = vcResolved.y + 80;
    for (const comp of vc.components) {
      map.set(`comp-${vc.name}-${comp.name}`, { x: 70, y: cy });
      cy += 120;
    }
    y += Math.max(vc.components.length * 120 + 100, 160);
  }
  return map;
}

export function modelToFlow(
  model: IfmlModel,
  prevPositions?: Map<string, XY>,
): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const fallback = gridPositions(model);

  for (const vc of model.viewContainers) {
    const vcId = `vc-${vc.name}`;
    const vcPos = vc.position ?? prevPositions?.get(vcId) ?? fallback.get(vcId)!;
    nodes.push({
      id: vcId,
      type: 'view-container',
      position: vcPos,
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

    for (const comp of vc.components) {
      const compId = `comp-${vc.name}-${comp.name}`;
      const compPos = comp.position ?? prevPositions?.get(compId) ?? fallback.get(compId)!;
      nodes.push({
        id: compId,
        type: 'view-component',
        position: compPos,
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
    }
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

// Maps node positions back onto the model. Nodes sitting exactly on their
// grid fallback are treated as unmoved and keep their model position
// (so plain renders don't pollute the DSL with spurious positions).
export function flowToModel(nodes: Node[], model: IfmlModel): IfmlModel {
  const fallback = gridPositions(model);
  const result: IfmlModel = {
    ...model,
    viewContainers: model.viewContainers.map(vc => {
      const updated: ViewContainerData = { ...vc, components: vc.components.map(c => ({ ...c })) };
      const vcId = `vc-${vc.name}`;
      const vcNode = nodes.find(n => n.id === vcId);
      if (vcNode && !positionsEqual(vcNode.position, fallback.get(vcId)!)) {
        updated.position = { x: vcNode.position.x, y: vcNode.position.y };
      }
      for (const comp of updated.components) {
        const compId = `comp-${vc.name}-${comp.name}`;
        const compNode = nodes.find(n => n.id === compId);
        if (compNode && !positionsEqual(compNode.position, fallback.get(compId)!)) {
          comp.position = { x: compNode.position.x, y: compNode.position.y };
        }
      }
      return updated;
    }),
  };
  return result;
}

export function positionsEqual(a: XY, b: XY, epsilon = 0.5): boolean {
  return Math.abs(a.x - b.x) < epsilon && Math.abs(a.y - b.y) < epsilon;
}

const LAYER_COLUMN_WIDTH = 320;
const LAYER_COLUMN_GAP = 40;
const LAYER_ROW_HEIGHT = 180;
const LAYER_ROW_GAP = 120;

// Deterministic layered (sweep) layout: top-to-bottom columns per BFS layer.
// No external deps, stable ordering (name-sorted within a layer).
export function computeLayeredLayout(model: IfmlModel): Map<string, XY> {
  const names = model.viewContainers.map(vc => vc.name);
  const byName = new Map(model.viewContainers.map(vc => [vc.name, vc]));

  const indegree = new Map<string, number>(names.map(n => [n, 0]));
  const out: Map<string, string[]> = new Map(names.map(n => [n, []]));
  const seen = new Set<string>();

  for (const edge of model.navigationEdges) {
    if (!byName.has(edge.sourceContainer) || !byName.has(edge.targetContainer)) continue;
    const key = `${edge.sourceContainer}\u0000${edge.targetContainer}`;
    if (seen.has(key)) continue;
    seen.add(key);
    indegree.set(edge.targetContainer, (indegree.get(edge.targetContainer) ?? 0) + 1);
    out.get(edge.sourceContainer)?.push(edge.targetContainer);
  }

  const layer = new Map<string, number>(names.map(n => [n, 0]));
  let frontier = names.filter(n => (indegree.get(n) ?? 0) === 0).sort();
  if (frontier.length === 0 && names.length > 0) {
    frontier = [...names].sort();
  }

  const placed = new Set<string>();
  let current = 0;
  while (frontier.length > 0) {
    const next: string[] = [];
    const ordered = [...frontier].sort();
    for (const name of ordered) {
      if (placed.has(name)) continue;
      placed.add(name);
      layer.set(name, current);
      for (const child of out.get(name) ?? []) {
        const childLayer = layer.get(child) ?? 0;
        if (childLayer <= current) layer.set(child, current + 1);
        indegree.set(child, (indegree.get(child) ?? 1) - 1);
        if ((indegree.get(child) ?? 0) <= 0) next.push(child);
      }
    }
    frontier = next;
    current += 1;
  }

  // Any stragglers (cycles) get appended after the last computed layer.
  const maxLayer = Math.max(0, ...layer.values());
  for (const name of [...names].sort()) {
    if (!placed.has(name)) {
      placed.add(name);
      layer.set(name, maxLayer + 1);
    }
  }

  const positions = new Map<string, XY>();
  const perLayer = new Map<number, string[]>();
  for (const name of names) {
    const l = layer.get(name) ?? 0;
    const list = perLayer.get(l) ?? [];
    list.push(name);
    perLayer.set(l, list);
  }

  for (const [l, list] of perLayer) {
    list.sort();
    list.forEach((name, idx) => {
      const x = l * (LAYER_COLUMN_WIDTH + LAYER_COLUMN_GAP);
      const y = idx * (LAYER_ROW_HEIGHT + LAYER_ROW_GAP) + 50;
      positions.set(`vc-${name}`, { x, y });
      const vc = byName.get(name);
      if (!vc) return;
      let cy = y + 80;
      for (const comp of vc.components) {
        positions.set(`comp-${name}-${comp.name}`, { x: x + 20, y: cy });
        cy += LAYER_ROW_HEIGHT;
      }
    });
  }

  return positions;
}
