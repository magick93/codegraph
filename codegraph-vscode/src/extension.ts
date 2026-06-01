import * as vscode from 'vscode';
import { registerCommands } from './commands/register';
import { LspStatusBar, CodegenStatusBar } from './status-bar';
import { IfmlCompletionProvider } from './completion/providers';
import { LspClient } from './lsp/client';

let lspClient: LspClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
    const lspStatusBar = new LspStatusBar();
    context.subscriptions.push(lspStatusBar);

    const codegenStatusBar = new CodegenStatusBar();
    context.subscriptions.push(codegenStatusBar);

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

    lspStatusBar.show();

    // Start LSP in background (safe even if vscode-languageclient isn't bundled)
    try {
        const client = new LspClient(context, lspStatusBar);
        lspClient = client;
        client.start();
    } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        lspStatusBar.setLspState('disconnected');
        console.warn(`IFML LSP client failed to start: ${msg}`);
    }
}

export function deactivate(): void {
    if (lspClient) {
        lspClient.stop();
    }
}
