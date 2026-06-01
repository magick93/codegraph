import * as vscode from 'vscode';

// ── IFML Business Model Types ─────────────────────────────────────

export interface CodegenConfig {
  targets: string[];
  outputDir: string;
  lastRun: string | null;
  frameworks: FrameworkInfo[];
}

export interface FrameworkInfo {
  id: string;
  label: string;
  description: string;
  available: boolean;
}

// ── IFML Business Model Types ─────────────────────────────────────

export interface IfmlModel {
  viewContainers: ViewContainerData[];
  actions: ActionDefData[];
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

export interface ActionDefData {
  name: string;
  properties: Record<string, string>;
  events: EventData[];
}

// ── Sync Protocol ─────────────────────────────────────────────────

export class SyncEngine {
  private panel: vscode.WebviewPanel;
  private documentUri: vscode.Uri;

  constructor(panel: vscode.WebviewPanel, uri: vscode.Uri) {
    this.panel = panel;
    this.documentUri = uri;
  }

  sendModel(model: IfmlModel): void {
    this.panel.webview.postMessage({
      command: 'sync/modelUpdate',
      model,
    });
  }

  sendSelection(elementId: string): void {
    this.panel.webview.postMessage({
      command: 'sync/selectElement',
      elementId,
    });
  }

  sendCodegenConfig(config: CodegenConfig) {
    this.panel.webview.postMessage({ command: 'sync/codegenConfig', config });
  }

  handleDiagramChange(handler: (model: IfmlModel) => void): void {
    this.panel.webview.onDidReceiveMessage((message) => {
      if (message.command === 'sync/diagramChanged') {
        handler(message.model as IfmlModel);
      }
    });
  }
}
