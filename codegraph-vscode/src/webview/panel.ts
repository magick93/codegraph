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

    panel.webview.html = getPlaceholderHtml();

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

function getPlaceholderHtml(): string {
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>IFML Diagram</title>
    <style>
        body {
            display: flex;
            align-items: center;
            justify-content: center;
            height: 100vh;
            margin: 0;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
            color: #888;
            background: #1e1e1e;
        }
        .placeholder {
            text-align: center;
            padding: 2em;
        }
        .placeholder h2 {
            margin-bottom: 0.5em;
            color: #ccc;
            font-weight: 300;
        }
        .placeholder p {
            font-size: 0.9em;
            color: #666;
            line-height: 1.6;
        }
        .placeholder .icon {
            font-size: 3em;
            margin-bottom: 0.5em;
        }
    </style>
</head>
<body>
    <div class="placeholder">
        <div class="icon">&#9678;</div>
        <h2>IFML Diagram Preview</h2>
        <p>The visual diagram editor is under construction.</p>
        <p>This panel will render the SvelteFlow-based IFML diagram.</p>
        <p>Edits to the .ifml file will sync here in real-time.</p>
    </div>
</body>
</html>`;
}
