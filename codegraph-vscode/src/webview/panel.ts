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

  const nonce = getNonce();
  const cspSource = panel.webview.cspSource;

  panel.webview.html = getWebviewHtml(nonce, cspSource, styleUri, scriptUri);

  const sync = new SyncEngine(panel, uri);

  // Wait for WebView to signal ready
  panel.webview.onDidReceiveMessage((msg) => {
    if (msg.command === 'sync/ready') {
      sendModelFromDocument(uri, sync);
    }
  });

  // Re-send on document changes
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
      const text = doc.getText();
      const model = parseIfmlForDiagram(text);
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
  nonce: string,
  cspSource: string,
  styleUri: vscode.Uri,
  scriptUri: vscode.Uri,
): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}' 'unsafe-eval'; img-src ${cspSource} data:;">
  <link rel="stylesheet" href="${styleUri}" />
</head>
<body>
  <div id="root"></div>
  <script nonce="${nonce}">
(function() {
  let el = document.createElement('div');
  el.id = 'error';
  el.style.cssText = 'display:none;padding:12px;color:red;font-family:monospace;white-space:pre-wrap';
  document.body.prepend(el);
  window.onerror = function(m, u, l, c) {
    el.style.display = 'block';
    el.textContent += '\\n' + m + ' (' + u + ':' + l + ')';
  };
  window.addEventListener('unhandledrejection', function(e) {
    el.style.display = 'block';
    el.textContent += '\\nPromise: ' + e.reason;
  });
})();
  </script>
  <script defer nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
}
