import * as vscode from 'vscode';
import { registerCommands } from './commands/register';
import { StatusBar } from './status-bar';
import { IfmlCompletionProvider } from './completion/providers';

let lspClient: { start(): void; stop(): void } | undefined;

export function activate(context: vscode.ExtensionContext): void {
    const statusBar = new StatusBar();
    context.subscriptions.push(statusBar);

    // Register commands FIRST so they exist even if LSP fails to start
    registerCommands(context, lspClient);

    // Completion provider
    context.subscriptions.push(
        vscode.languages.registerCompletionItemProvider(
            { language: 'ifml' },
            new IfmlCompletionProvider(),
            ':', '"', '.', ' '
        )
    );

    statusBar.show();

    // Dynamically import LSP client so vscode-languageclient isn't loaded
    // at module load time (it's excluded from the VSIX package)
    startLspClient(context, statusBar).catch((err) => {
        const msg = err instanceof Error ? err.message : String(err);
        statusBar.setLspState('disconnected');
        console.warn(`IFML LSP client failed to start: ${msg}`);
    });
}

async function startLspClient(context: vscode.ExtensionContext, statusBar: StatusBar): Promise<void> {
    const { LspClient } = await import('./lsp/client');
    const client = new LspClient(context, statusBar);
    lspClient = client;
    client.start();
}

export function deactivate(): void {
    if (lspClient) {
        lspClient.stop();
    }
}
