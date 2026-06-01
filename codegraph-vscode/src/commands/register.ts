import * as vscode from 'vscode';
import * as path from 'path';
import { openDiagramPanel } from '../webview/panel';
import { LspClient } from '../lsp/client';
import { findBinaryPath } from '../binary';

export function registerCommands(context: vscode.ExtensionContext, lspClient: LspClient | undefined): void {
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
        vscode.commands.registerCommand('ifml.selectCodegenTargets', async () => {
            const config = vscode.workspace.getConfiguration('ifml.codegen');
            const current = config.get<string[]>('targets', ['svelte']);

            const frameworks: { id: string; label: string; description: string }[] = [
                { id: 'svelte', label: 'SvelteKit', description: 'Generates SvelteKit routes, load functions, and navigation helpers' },
                { id: 'react', label: 'Next.js (React)', description: 'Generates Next.js App Router pages, server components, and route config' },
                { id: 'vue', label: 'Vue/Nuxt', description: 'Generates Nuxt 3 pages and Vue Router configuration' },
                { id: 'flutter', label: 'Flutter', description: 'Generates Flutter screens, forms, and named route config' },
                { id: 'swiftui', label: 'SwiftUI', description: 'Generates SwiftUI views and NavigationStack configuration' },
            ];

            const items: (vscode.QuickPickItem & { id: string })[] = frameworks.map(f => ({
                id: f.id,
                label: `${current.includes(f.id) ? '$(check)' : '$(circle)'} ${f.label}`,
                description: f.description,
                picked: current.includes(f.id),
            }));

            const selected = await vscode.window.showQuickPick(items, {
                canPickMany: true,
                placeHolder: 'Select code generation targets',
                title: 'IFML Code Generation Targets',
            });

            if (selected) {
                await config.update('targets', selected.map(s => s.id), vscode.ConfigurationTarget.Workspace);
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('ifml.generate', async () => {
            const workspaceFolders = vscode.workspace.workspaceFolders;
            if (!workspaceFolders) {
                vscode.window.showErrorMessage('Open a workspace folder first');
                return;
            }
            const workspaceRoot = workspaceFolders[0].uri.fsPath;
            const config = vscode.workspace.getConfiguration('ifml');
            const codegenConfig = vscode.workspace.getConfiguration('ifml.codegen');

            const targets = codegenConfig.get<string[]>('targets', ['svelte']);
            const outputDir = codegenConfig.get<string>('outputDir', 'generated');
            const schemaDirs = config.get<string[]>('schemaDirs', ['schemas']);
            const classifierPath = config.get<string>('classifierConfig', 'classifier.toml');
            const domainConfig = config.get<string>('domainConfig', 'domains.toml');

            await vscode.workspace.saveAll();

            const ifmlFiles = await vscode.workspace.findFiles('**/*.ifml', '**/node_modules/**');

            const schemaArgs = schemaDirs.map(d => `--schemas ${path.resolve(workspaceRoot, d)}`).join(' ');
            const ifmlArgs = ifmlFiles.map(f => `--ifml-files ${f.fsPath}`).join(' ');
            const targetArgs = targets.map(t => `--ifml-framework ${t}`).join(' ');

            // Find codegraph binary using same discovery as LSP
            const configuredPath = config.get<string>('codegraphPath', 'codegraph');
            const found = findBinaryPath(configuredPath, context.extensionUri.fsPath);

            let cmd: string;
            let cwd: string | undefined;
            if (found.binary === 'cargo') {
                cmd = 'cargo run -p codegraph -- run';
                cwd = found.cwd;
            } else {
                cmd = `${found.binary} run`;
                cwd = workspaceRoot;
            }

            const fullCmd = `${cmd} ${schemaArgs} --classifier ${workspaceRoot}/${classifierPath} --config ${workspaceRoot}/${domainConfig} ${ifmlArgs} ${targetArgs} --output ${workspaceRoot}/${outputDir}`;

            const terminal = vscode.window.createTerminal({ name: 'codegraph', cwd: cwd || workspaceRoot });
            terminal.show();
            terminal.sendText(fullCmd);

            await context.workspaceState.update('codegenLastRun', new Date().toISOString());
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
