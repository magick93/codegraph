import * as vscode from 'vscode';

export class LspStatusBar {
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

export class CodegenStatusBar {
    private item: vscode.StatusBarItem;

    constructor() {
        this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 99);
        this.item.command = 'ifml.selectCodegenTargets';
        this.item.tooltip = 'IFML code generation targets — click to change';
        this.update();
        vscode.workspace.onDidChangeConfiguration(e => {
            if (e.affectsConfiguration('ifml.codegen.targets')) {
                this.update();
            }
        });
    }

    update() {
        const targets = vscode.workspace.getConfiguration('ifml.codegen').get<string[]>('targets', ['svelte']);
        const labels: Record<string, string> = {
            svelte: 'SvelteKit', react: 'Next.js', vue: 'Vue/Nuxt', flutter: 'Flutter', swiftui: 'SwiftUI'
        };
        const parts = targets.map(t => labels[t] || t);
        this.item.text = `$(tools) IFML: ${parts.join(', ')}`;
        this.item.show();
    }

    dispose(): void {
        this.item.dispose();
    }
}
