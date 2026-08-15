import { describe, it, expect } from 'vitest';
import { modelToFlow, flowToModel, positionsEqual, computeLayeredLayout } from './layout';
import type { IfmlModel, Node } from './types';

function makeModel(overrides?: Partial<IfmlModel>): IfmlModel {
  return {
    viewContainers: [
      {
        name: 'List',
        label: 'Customer List',
        isXor: false,
        isDefault: false,
        isLandmark: true,
        isModal: false,
        params: [],
        components: [
          { name: 'grid', componentType: 'list', entity: 'Customer', fields: ['name'], properties: {}, events: [], parts: [] },
          { name: 'search', componentType: 'form', entity: 'Customer', fields: [], properties: {}, events: [], parts: [] },
        ],
        events: [],
        containers: [],
      },
      {
        name: 'Detail',
        label: undefined,
        isXor: false,
        isDefault: false,
        isLandmark: false,
        isModal: false,
        params: [{ name: 'id', typeRef: 'Uuid' }],
        components: [],
        events: [],
        containers: [],
      },
    ],
    actions: [],
    navigationEdges: [{ sourceContainer: 'List', sourceEvent: 'select', targetContainer: 'Detail' }],
    dataFlows: [],
    generationOrder: ['List', 'Detail'],
    ...overrides,
  };
}

function asFlowNode(n: Node) {
  return { id: n.id, position: n.position, data: n.data } as Node;
}

describe('modelToFlow', () => {
  it('uses persisted position over grid fallback', () => {
    const model = makeModel();
    model.viewContainers[0].position = { x: 120, y: 240 };
    model.viewContainers[0].components[0].position = { x: 200, y: 300 };
    const { nodes } = modelToFlow(model);
    const vc = nodes.find(n => n.id === 'vc-List')!;
    const comp = nodes.find(n => n.id === 'comp-List-grid')!;
    expect(vc.position).toEqual({ x: 120, y: 240 });
    expect(comp.position).toEqual({ x: 200, y: 300 });
  });

  it('falls back to grid positions when no position present', () => {
    const { nodes } = modelToFlow(makeModel());
    const vc = nodes.find(n => n.id === 'vc-List')!;
    const vc2 = nodes.find(n => n.id === 'vc-Detail')!;
    const comp = nodes.find(n => n.id === 'comp-List-grid')!;
    expect(vc.position).toEqual({ x: 50, y: 50 });
    expect(comp.position).toEqual({ x: 70, y: 130 });
    // second container below first: 2 comps => 120*2+100 = 340; 50+340 = 390
    expect(vc2.position).toEqual({ x: 50, y: 390 });
  });

  it('prefers prev-position map for surviving ids', () => {
    const prev = new Map<string, { x: number; y: number }>([
      ['vc-List', { x: 500, y: 600 }],
      ['comp-List-grid', { x: 550, y: 650 }],
    ]);
    const { nodes } = modelToFlow(makeModel(), prev);
    expect(nodes.find(n => n.id === 'vc-List')!.position).toEqual({ x: 500, y: 600 });
    expect(nodes.find(n => n.id === 'comp-List-grid')!.position).toEqual({ x: 550, y: 650 });
    // grid fallback for ids not in the map
    expect(nodes.find(n => n.id === 'vc-Detail')!.position).toEqual({ x: 50, y: 390 });
  });

  it('builds navigation edges with container ids', () => {
    const { edges } = modelToFlow(makeModel());
    expect(edges).toHaveLength(1);
    expect(edges[0].source).toBe('vc-List');
    expect(edges[0].target).toBe('vc-Detail');
  });
});

describe('flowToModel', () => {
  it('maps node positions back onto the model', () => {
    const model = makeModel();
    const { nodes } = modelToFlow(model);
    nodes.find(n => n.id === 'vc-List')!.position = { x: 321.5, y: 122.25 };
    nodes.find(n => n.id === 'comp-List-grid')!.position = { x: 400, y: 200 };
    const updated = flowToModel(nodes.map(asFlowNode), model);
    expect(updated.viewContainers[0].position?.x).toBeCloseTo(321.5, 4);
    expect(updated.viewContainers[0].position?.y).toBeCloseTo(122.25, 4);
    expect(updated.viewContainers[0].components[0].position?.x).toBeCloseTo(400, 4);
    expect(updated.viewContainers[0].components[0].position?.y).toBeCloseTo(200, 4);
    // untouched container keeps its original position
    expect(updated.viewContainers[1].position).toBeUndefined();
  });

  it('keeps model position when node is missing', () => {
    const model = makeModel();
    model.viewContainers[0].position = { x: 10, y: 20 };
    const { nodes } = modelToFlow(model);
    const withoutVc = nodes.filter(n => n.id !== 'vc-List' && !n.id.startsWith('comp-List-'));
    const updated = flowToModel(withoutVc.map(asFlowNode), model);
    expect(updated.viewContainers[0].position).toEqual({ x: 10, y: 20 });
    expect(updated.viewContainers[0].components[0].position).toBeUndefined();
  });

  it('round-trips through modelToFlow', () => {
    const model = makeModel();
    model.viewContainers[0].position = { x: 100, y: 200 };
    const { nodes } = modelToFlow(model);
    nodes.find(n => n.id === 'vc-List')!.position = { x: 100.25, y: 199.75 };
    const updated = flowToModel(nodes.map(asFlowNode), model);
    expect(updated.viewContainers[0].position?.x).toBeCloseTo(100.25, 4);
    expect(updated.viewContainers[0].position?.y).toBeCloseTo(199.75, 4);
  });
});

describe('positionsEqual', () => {
  it('returns true within epsilon', () => {
    expect(positionsEqual({ x: 1, y: 2 }, { x: 1.4, y: 2.4 }, 0.5)).toBe(true);
    expect(positionsEqual({ x: 1, y: 2 }, { x: 1.6, y: 2 }, 0.5)).toBe(false);
  });
});

describe('computeLayeredLayout', () => {
  it('puts downstream views in later columns', () => {
    const model = makeModel();
    model.navigationEdges = [
      { sourceContainer: 'List', sourceEvent: 'select', targetContainer: 'Detail' },
    ];
    const layout = computeLayeredLayout(model);
    const list = layout.get('vc-List')!;
    const detail = layout.get('vc-Detail')!;
    expect(detail.x).toBeGreaterThan(list.x);
    expect(list.x).toBe(0);
    expect(detail.x).toBe(360); // 320 + 40 column gap
  });

  it('stacks siblings vertically with spacing', () => {
    const model = makeModel();
    model.viewContainers.push({
      name: 'A1', label: undefined, isXor: false, isDefault: false, isLandmark: false,
      isModal: false, params: [], components: [], events: [], containers: [],
    });
    model.viewContainers.push({
      name: 'A2', label: undefined, isXor: false, isDefault: false, isLandmark: false,
      isModal: false, params: [], components: [], events: [], containers: [],
    });
    model.viewContainers.push({
      name: 'A3', label: undefined, isXor: false, isDefault: false, isLandmark: false,
      isModal: false, params: [], components: [], events: [], containers: [],
    });
    const layout = computeLayeredLayout(model);
    const y1 = layout.get('vc-A1')!.y;
    const y2 = layout.get('vc-A2')!.y;
    const y3 = layout.get('vc-A3')!.y;
    expect(y2 - y1).toBe(300); // 180 + 120 row spacing
    expect(y3 - y2).toBe(300);
  });

  it('is deterministic', () => {
    const a = computeLayeredLayout(makeModel());
    const b = computeLayeredLayout(makeModel());
    expect([...a.entries()]).toEqual([...b.entries()]);
  });
});
