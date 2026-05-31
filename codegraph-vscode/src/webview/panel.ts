import * as vscode from 'vscode';
import { SyncEngine } from './sync';

const panels = new Map<string, vscode.WebviewPanel>();

export function openDiagramPanel(context: vscode.ExtensionContext, uri: vscode.Uri): void {
    const key = uri.toString();
    const existing = panels.get(key);
    if (existing) {
        existing.reveal(vscode.ViewColumn.Beside);
        return;
    }

    const panel = vscode.window.createWebviewPanel(
        'ifmlDiagram',
        `IFML Diagram: ${uri.path.split('/').pop()}`,
        vscode.ViewColumn.Beside,
        {
            enableScripts: true,
            localResourceRoots: [
                vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview'),
            ],
            retainContextWhenHidden: true,
        }
    );

    const scriptUri = panel.webview.asWebviewUri(
        vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview', 'ifml-diagram.js')
    );
    const styleUri = panel.webview.asWebviewUri(
        vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview', 'ifml-diagram.css')
    );

    panel.webview.html = getWebviewHtml(scriptUri, styleUri, panel);

    const sync = new SyncEngine(panel, uri);

    const watcher = vscode.workspace.onDidChangeTextDocument((e) => {
        if (e.document.uri.toString() === uri.toString()) {
            panel.webview.postMessage({
                command: 'documentChanged',
                text: e.document.getText(),
            });
        }
    });

    panel.onDidDispose(() => {
        watcher.dispose();
        panels.delete(key);
    });

    panels.set(key, panel);
}

function getNonce(): string {
    let text = '';
    const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    for (let i = 0; i < 64; i++) {
        text += possible.charAt(Math.floor(Math.random() * possible.length));
    }
    return text;
}

function getWebviewHtml(
    scriptUri: vscode.Uri,
    styleUri: vscode.Uri,
    panel: vscode.WebviewPanel,
): string {
    const nonce = getNonce();
    const cspSource = panel.webview.cspSource;
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta http-equiv="Content-Security-Policy" content="
        default-src 'none';
        style-src ${cspSource} 'unsafe-inline';
        script-src 'nonce-${nonce}' 'wasm-unsafe-eval';
        font-src ${cspSource};
    " />
    <link rel="stylesheet" href="${styleUri}" />
</head>
<body>
    <div id="root"></div>
    <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
}
