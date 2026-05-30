import * as vscode from 'vscode';

export interface IfmlNode {
    id: string;
    type: 'view-container' | 'view-component';
    label: string;
    position: { x: number; y: number };
    data: Record<string, unknown>;
}

export interface IfmlEdge {
    id: string;
    source: string;
    target: string;
    type: string;
    label?: string;
    data: Record<string, unknown>;
}

export interface IfmlModel {
    nodes: IfmlNode[];
    edges: IfmlEdge[];
}

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

    handleDiagramChange(handler: (model: IfmlModel) => void): void {
        this.panel.webview.onDidReceiveMessage((message) => {
            if (message.command === 'sync/diagramChanged') {
                handler(message.model as IfmlModel);
            }
        });
    }

    handleRequestFullModel(handler: () => IfmlModel): void {
        this.panel.webview.onDidReceiveMessage((message) => {
            if (message.command === 'sync/requestFull') {
                const model = handler();
                this.sendModel(model);
            }
        });
    }
}
