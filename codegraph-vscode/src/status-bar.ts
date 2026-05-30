import * as vscode from 'vscode';

export class StatusBar {
    private item: vscode.StatusBarItem;

    constructor() {
        this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
        this.item.text = '$(hubot) IFML: ...';
        this.item.tooltip = 'IFML Language Server status';
        this.item.command = 'ifml.refreshLsp';
    }

    show(): void {
        this.item.show();
    }

    dispose(): void {
        this.item.dispose();
    }

    setLspState(state: 'starting' | 'running' | 'disconnected'): void {
        const icons: Record<string, string> = {
            starting: '$(sync~spin)',
            running: '$(check)',
            disconnected: '$(error)',
        };
        this.item.text = `${icons[state] || '$(question)'} IFML: ${state}`;
        this.item.tooltip = `IFML Language Server: ${state}`;
    }
}
