import * as vscode from 'vscode';
import { LspClient } from './lsp/client';
import { registerCommands } from './commands/register';
import { StatusBar } from './status-bar';
import { IfmlCompletionProvider } from './completion/providers';

let lspClient: LspClient | undefined;

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

    // Start LSP client in background (may fail if codegraph binary not found)
    try {
        lspClient = new LspClient(context, statusBar);
        lspClient.start();
    } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        statusBar.setLspState('disconnected');
        console.warn(`IFML LSP client failed to start: ${msg}`);
    }
}

export function deactivate(): void {
    if (lspClient) {
        lspClient.stop();
    }
}
