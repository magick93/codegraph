export interface IfmlModel {
  viewContainers: ViewContainerData[];
  actions: ActionData[];
  navigationEdges: NavigationEdgeData[];
  dataFlows: DataFlowData[];
  generationOrder: string[];
}

export interface ViewContainerData {
  name: string;
  label?: string;
  isXor: boolean;
  isDefault: boolean;
  isLandmark: boolean;
  isModal: boolean;
  params: ParameterDef[];
  components: ViewComponentData[];
  events: EventData[];
  containers: ViewContainerData[];
}

export interface ViewComponentData {
  name: string;
  componentType: string;
  mode?: string;
  entity?: string;
  fields: string[];
  filter?: string;
  properties: Record<string, string>;
  events: EventData[];
  parts: ComponentPartData[];
}

export interface ComponentPartData {
  name: string;
  role: string;
}

export interface EventData {
  name: string;
  eventType: string;
  params: string[];
  action: ActionData;
}

export type ActionData =
  | { type: 'navigate'; target: string; binding?: Record<string, string> }
  | { type: 'refresh'; target: string; binding?: Record<string, string> }
  | { type: 'action'; name: string }
  | { type: 'stay' };

export interface ParameterDef {
  name: string;
  typeRef: string;
}

export interface NavigationEdgeData {
  sourceContainer: string;
  sourceEvent: string;
  targetContainer: string;
  parameterBinding?: Record<string, string>;
  conditionalExpression?: string;
}

export interface DataFlowData {
  sourceElement: string;
  targetElement: string;
  sourceParam?: string;
  targetParam?: string;
}

export type SyncMessage =
  | { command: 'sync/modelUpdate'; model: IfmlModel }
  | { command: 'sync/selectElement'; elementId: string }
  | { command: 'sync/diagramChanged'; model: IfmlModel }
  | { command: 'sync/selectInText'; elementId: string }
  | { command: 'documentChanged'; text: string };
