import * as vscode from 'vscode';
import { openDiagramPanel } from '../webview/panel';

export interface LspClientLike {
    start(): void;
    stop(): void;
    restart?(): Promise<void>;
}

export function registerCommands(context: vscode.ExtensionContext, lspClient: LspClientLike | undefined): void {
    context.subscriptions.push(
        vscode.commands.registerCommand('ifml.openDiagram', () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'ifml') {
                vscode.window.showErrorMessage('Open an .ifml file first');
                return;
            }
            openDiagramPanel(context, editor.document.uri);
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('ifml.validate', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'ifml') {
                vscode.window.showErrorMessage('Open an .ifml file first');
                return;
            }
            await vscode.commands.executeCommand('lsp.forceDiagnosticRefresh');
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('ifml.generate', async () => {
            const workspaceFolders = vscode.workspace.workspaceFolders;
            if (!workspaceFolders) {
                vscode.window.showErrorMessage('Open a workspace folder first');
                return;
            }
            const config = vscode.workspace.getConfiguration('ifml');
            const schemaDirs = config.get<string[]>('schemaDirs', ['schemas']);
            const classifierPath = config.get<string>('classifierConfig', 'classifier.toml');
            const domainConfig = config.get<string>('domainConfig', 'domains.toml');

            await vscode.workspace.saveAll();

            const terminal = vscode.window.createTerminal('codegraph');
            terminal.show();
            const schemaArgs = schemaDirs.map(d => `--schemas ${d}`).join(' ');
            terminal.sendText(
                `cargo run -- run ${schemaArgs} --classifier ${classifierPath} --config ${domainConfig} --output generated`
            );
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('ifml.refreshLsp', async () => {
            if (lspClient?.restart) {
                await lspClient.restart();
                vscode.window.showInformationMessage('IFML Language Server restarted');
            } else {
                vscode.window.showErrorMessage('IFML Language Server not available');
            }
        })
    );
}
