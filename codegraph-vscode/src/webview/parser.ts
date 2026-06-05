import type {
  IfmlModel, ViewContainerData, ViewComponentData,
  EventData, ActionData, NavigationEdgeData, ParameterDef
} from './sync';

/// Lightweight IFML text extractor for diagram rendering.
/// Uses a simple brace-depth parser to handle nested blocks.

function extractBlock(text: string, start: number): { content: string; end: number } | null {
  if (text[start] !== '{') return null;
  let depth = 0;
  let i = start;
  while (i < text.length) {
    if (text[i] === '{') depth++;
    else if (text[i] === '}') {
      depth--;
      if (depth === 0) return { content: text.slice(start + 1, i), end: i + 1 };
    }
    i++;
  }
  return null;
}

function* scanBlocks(text: string, keyword: string): Generator<{ name: string; body: string }> {
  const re = new RegExp(`${keyword}\\s+"([^"]+)"\\s*\\{`, 'gs');
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    const name = match[1];
    const block = extractBlock(text, match.index + match[0].length - 1);
    if (block) {
      yield { name, body: block.content };
      re.lastIndex = block.end;
    }
  }
}

export function parseIfmlForDiagram(text: string): IfmlModel {
  const viewContainers: ViewContainerData[] = [];

  for (const v of scanBlocks(text, 'view')) {
    viewContainers.push(extractViewContainer(v.name, v.body));
  }

  const navigationEdges = extractNavigationEdges(viewContainers);

  return {
    viewContainers,
    actions: [],
    navigationEdges,
    dataFlows: [],
    generationOrder: viewContainers.map(v => v.name),
  };
}

function extractViewContainer(name: string, body: string): ViewContainerData {
  const components: ViewComponentData[] = [];
  const events: EventData[] = [];

  for (const c of scanBlocks(body, 'component')) {
    components.push(extractComponent(c.name, c.body));
  }

  // View-level events (on load, etc.)
  const viewEventRe = /on\s+(\w+)\s*\(([^)]*)\)?\s*->\s*(\w+)\s*\(\s*"([^"]*)"\s*(?:,\s*\{[^}]*\})?\s*\)/g;
  for (const m of body.matchAll(viewEventRe)) {
    if (m[1] === 'select' || m[1] === 'submit' || m[1] === 'save' || m[1] === 'edit') continue;
    events.push(makeEvent(m, name, 'view'));
  }

  const labelMatch = /label\s*:\s*"([^"]+)"/.exec(body);

  return {
    name,
    label: labelMatch?.[1],
    isXor: /xor\s*:\s*true/i.test(body),
    isDefault: /default\s*:\s*true/i.test(body),
    isLandmark: /landmark\s*:\s*true/i.test(body),
    isModal: /modal\s*:\s*true/i.test(body),
    params: extractParams(body),
    components,
    events,
    containers: [],
  };
}

function extractComponent(name: string, body: string): ViewComponentData {
  const typeMatch = /type\s*:\s*(\w+)/.exec(body);
  const dataMatch = /data\s*:\s*(\w+)/.exec(body);
  const modeMatch = /mode\s*:\s*(\w+)/.exec(body);
  const fieldsMatch = /fields\s*:\s*\[([^\]]*)\]/.exec(body);
  const filterMatch = /filter\s*:\s*([^;]+)/.exec(body);

  const fields = fieldsMatch
    ? fieldsMatch[1].split(',').map(f => f.trim()).filter(Boolean)
    : [];

  const events: EventData[] = [];
  const eventRe = /on\s+(\w+)\s*\(([^)]*)\)?\s*->\s*(\w+)\s*\(\s*"([^"]*)"\s*(?:,\s*\{[^}]*\})?\s*\)/g;
  for (const m of body.matchAll(eventRe)) {
    events.push(makeEvent(m, name, 'comp'));
  }

  return {
    name,
    componentType: typeMatch?.[1] ?? 'unknown',
    mode: modeMatch?.[1],
    entity: dataMatch?.[1],
    fields,
    filter: filterMatch?.[1]?.trim(),
    properties: { ...(typeMatch && { type: typeMatch[1] }), ...(modeMatch && { mode: modeMatch[1] }) },
    events,
    parts: [],
  };
}

function makeEvent(m: RegExpExecArray | string[], parent: string, kind: string): EventData {
  const eventType = m[1];
  const params = m[2] ? m[2].split(',').map(p => p.trim()).filter(Boolean) : [];
  const actionType = m[3];
  const actionTarget = m[4];

  let action: ActionData;
  if (actionType === 'navigate') action = { type: 'navigate', target: actionTarget };
  else if (actionType === 'refresh') action = { type: 'refresh', target: actionTarget };
  else if (actionType === 'action') action = { type: 'action', name: actionTarget };
  else action = { type: 'stay' };

  return {
    name: `${kind}_${parent}_${eventType}`,
    eventType,
    params,
    action,
  };
}

function extractParams(body: string): ParameterDef[] {
  const result: ParameterDef[] = [];
  const m = /params\s*\{([^}]*)\}/.exec(body);
  if (m) {
    for (const pair of m[1].split(',').map(s => s.trim()).filter(Boolean)) {
      const [name, typeRef] = pair.split(':').map(s => s.trim());
      if (name && typeRef) result.push({ name, typeRef });
    }
  }
  return result;
}

function extractNavigationEdges(containers: ViewContainerData[]): NavigationEdgeData[] {
  const edges: NavigationEdgeData[] = [];
  for (const vc of containers) {
    for (const comp of vc.components) {
      for (const evt of comp.events) {
        if (evt.action.type === 'navigate' && evt.action.target) {
          edges.push({ sourceContainer: vc.name, sourceEvent: evt.eventType, targetContainer: evt.action.target });
        }
      }
    }
    for (const evt of vc.events) {
      if (evt.action.type === 'navigate' && evt.action.target) {
        edges.push({ sourceContainer: vc.name, sourceEvent: evt.eventType, targetContainer: evt.action.target });
      }
    }
  }
  return edges;
}
