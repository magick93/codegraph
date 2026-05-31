import type {
  IfmlModel, ViewContainerData, ViewComponentData,
  EventData, ActionData, NavigationEdgeData, ParameterDef
} from './sync';

/// Lightweight IFML text extractor for diagram rendering.
/// Uses regex to extract views, components, events, and navigation flows.
/// This is not a full parser — it only extracts the structures needed for
/// the visual diagram. Structural validation is done by the Rust parser.

export function parseIfmlForDiagram(text: string): IfmlModel {
  const viewContainers: ViewContainerData[] = [];

  // Match: view "Name" { ... } — simple non-nested brace matching
  const viewRegex = /view\s+"([^"]+)"\s*\{([^}]*)\}/gs;
  let viewMatch: RegExpExecArray | null;

  while ((viewMatch = viewRegex.exec(text)) !== null) {
    const viewName = viewMatch[1];
    const viewBody = viewMatch[2];
    const container = extractViewContainer(viewName, viewBody);
    viewContainers.push(container);
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
  const isLandmark = /landmark\s*:\s*true/i.test(body);
  const isModal = /modal\s*:\s*true/i.test(body);
  const labelMatch = /label\s*:\s*"([^"]+)"/.exec(body);
  const params = extractParams(body);
  const components: ViewComponentData[] = [];
  const events: EventData[] = [];

  // Extract component blocks
  // Match: component "Name" { ... }
  const compRegex = /component\s+"([^"]+)"\s*\{([^}]*)\}/gs;
  let compMatch: RegExpExecArray | null;

  while ((compMatch = compRegex.exec(body)) !== null) {
    components.push(extractComponent(compMatch[1], compMatch[2]));
  }

  // View-level events (on load, etc.)
  const viewEventRegex = /on\s+(\w+)\s*\(?([^)]*)\)?\s*->\s*(\w+)\s*\(\s*"([^"]*)"\s*(?:,\s*\{[^}]*\})?\s*\)/g;
  let evtMatch: RegExpExecArray | null;
  while ((evtMatch = viewEventRegex.exec(body)) !== null) {
    if (!evtMatch[1] || evtMatch[1] === 'select' || evtMatch[1] === 'submit' ||
        evtMatch[1] === 'save' || evtMatch[1] === 'edit') continue; // skip component events
    events.push(makeEvent(evtMatch, name, 'view'));
  }

  return {
    name,
    label: labelMatch ? labelMatch[1] : undefined,
    isXor: /xor\s*:\s*true/i.test(body),
    isDefault: /default\s*:\s*true/i.test(body),
    isLandmark,
    isModal,
    params,
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

  const properties: Record<string, string> = {};
  if (typeMatch) properties.type = typeMatch[1];
  if (modeMatch) properties.mode = modeMatch[1];

  const events: EventData[] = [];
  const eventRegex = /on\s+(\w+)\s*\(?([^)]*)\)?\s*->\s*(\w+)\s*\(\s*"([^"]*)"\s*(?:,\s*\{[^}]*\})?\s*\)/g;
  let evtMatch: RegExpExecArray | null;
  while ((evtMatch = eventRegex.exec(body)) !== null) {
    events.push(makeEvent(evtMatch, name, 'comp'));
  }

  return {
    name,
    componentType: typeMatch ? typeMatch[1] : 'unknown',
    mode: modeMatch ? modeMatch[1] : undefined,
    entity: dataMatch ? dataMatch[1] : undefined,
    fields,
    filter: filterMatch ? filterMatch[1].trim() : undefined,
    properties,
    events,
    parts: [],
  };
}

function makeEvent(m: RegExpExecArray, parent: string, kind: string): EventData {
  const eventType = m[1];
  const params = m[2] ? m[2].split(',').map(p => p.trim()).filter(Boolean) : [];
  const actionType = m[3];
  const actionTarget = m[4];

  let action: ActionData;
  if (actionType === 'navigate') {
    action = { type: 'navigate', target: actionTarget };
  } else if (actionType === 'refresh') {
    action = { type: 'refresh', target: actionTarget };
  } else if (actionType === 'action') {
    action = { type: 'action', name: actionTarget };
  } else {
    action = { type: 'stay' };
  }

  return {
    name: `${kind}_${parent}_${eventType}`,
    eventType,
    params,
    action,
  };
}

function extractParams(body: string): ParameterDef[] {
  const params: ParameterDef[] = [];
  const paramRegex = /params\s*\{([^}]*)\}/.exec(body);
  if (paramRegex) {
    const pairs = paramRegex[1].split(',').map(p => p.trim()).filter(Boolean);
    for (const pair of pairs) {
      const [name, typeRef] = pair.split(':').map(s => s.trim());
      if (name && typeRef) params.push({ name, typeRef });
    }
  }
  return params;
}

function extractNavigationEdges(containers: ViewContainerData[]): NavigationEdgeData[] {
  const edges: NavigationEdgeData[] = [];
  for (const vc of containers) {
    for (const comp of vc.components) {
      for (const evt of comp.events) {
        if (evt.action.type === 'navigate' && evt.action.target) {
          edges.push({
            sourceContainer: vc.name,
            sourceEvent: evt.eventType,
            targetContainer: evt.action.target,
          });
        }
      }
    }
    for (const evt of vc.events) {
      if (evt.action.type === 'navigate' && evt.action.target) {
        edges.push({
          sourceContainer: vc.name,
          sourceEvent: evt.eventType,
          targetContainer: evt.action.target,
        });
      }
    }
  }
  return edges;
}
