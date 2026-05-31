import * as vscode from 'vscode';
import { SyncEngine } from './sync';
import { parseIfmlForDiagram } from './parser';

const panels = new Map<string, { panel: vscode.WebviewPanel; sync: SyncEngine }>();

export function openDiagramPanel(context: vscode.ExtensionContext, uri: vscode.Uri): void {
  const key = uri.toString();
  const existing = panels.get(key);
  if (existing) {
    existing.panel.reveal(vscode.ViewColumn.Beside);
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

  // Wait for WebView to signal ready, then send the parsed model
  let webviewReady = false;
  panel.webview.onDidReceiveMessage((msg) => {
    if (msg.command === 'sync/ready') {
      webviewReady = true;
      sendModelFromDocument(uri, sync);
    }
  });

  // Also send on document changes (panel is already open, so WebView is ready)
  const watcher = vscode.workspace.onDidChangeTextDocument((e) => {
    if (e.document.uri.toString() === uri.toString()) {
      sendModelFromDocument(e.document.uri, sync);
    }
  });

  panel.onDidDispose(() => {
    watcher.dispose();
    panels.delete(key);
  });

  panels.set(key, { panel, sync });
}

function sendModelFromDocument(uri: vscode.Uri, sync: SyncEngine): void {
  try {
    const doc = vscode.workspace.textDocuments.find(d => d.uri.toString() === uri.toString());
    if (doc) {
      const model = parseIfmlForDiagram(doc.getText());
      sync.sendModel(model);
    }
  } catch (err) {
    console.error('IFML parse error:', err);
  }
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
        script-src 'nonce-${nonce}' 'unsafe-eval';
        img-src ${cspSource} data:;
        font-src ${cspSource};
        connect-src 'self' ${cspSource} ws:;
    " />
    <link rel="stylesheet" href="${styleUri}" />
</head>
<body>
    <div id="root"></div>
    <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
}
