import * as vscode from 'vscode';
import { LspClient } from './lsp/client';
import { registerCommands } from './commands/register';
import { StatusBar } from './status-bar';
import { IfmlCompletionProvider } from './completion/providers';

let lspClient: LspClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
    const statusBar = new StatusBar();
    context.subscriptions.push(statusBar);

    lspClient = new LspClient(context, statusBar);
    lspClient.start();

    registerCommands(context, lspClient);

    context.subscriptions.push(
        vscode.languages.registerCompletionItemProvider(
            { language: 'ifml' },
            new IfmlCompletionProvider(),
            ':', '"', '.', ' '
        )
    );

    statusBar.show();
}

export function deactivate(): void {
    if (lspClient) {
        lspClient.stop();
    }
}
