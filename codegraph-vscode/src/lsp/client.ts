import * as vscode from 'vscode';
import { ChildProcess, spawn } from 'child_process';
import { StatusBar } from '../status-bar';

export class LspClient {
    private proc: ChildProcess | null = null;
    private outputChannel: vscode.OutputChannel;

    constructor(
        private context: vscode.ExtensionContext,
        private statusBar: StatusBar
    ) {
        this.outputChannel = vscode.window.createOutputChannel('IFML Language Server');
    }

    start(): void {
        // Dynamically import vscode-languageclient to avoid requiring it
        // at module load time (it's excluded from the VSIX package)
        import('vscode-languageclient/node').then(({ LanguageClient, State }) => {
            const config = vscode.workspace.getConfiguration('ifml');
            const binaryPath = config.get<string>('codegraphPath', 'codegraph');
            const schemaDirs = config.get<string[]>('schemaDirs', ['schemas']);
            const classifierPath = config.get<string>('classifierConfig', 'classifier.toml');
            const domainConfig = config.get<string>('domainConfig', 'domains.toml');

            const args = ['lsp'];
            for (const dir of schemaDirs) {
                args.push('--schemas', dir);
            }
            args.push('--classifier', classifierPath);
            args.push('--config', domainConfig);

            this.statusBar.setLspState('starting');

            const serverOptions = () => {
                return new Promise<{ reader: NodeJS.ReadableStream; writer: NodeJS.WritableStream }>((resolve, reject) => {
                    const child: ChildProcess = spawn(binaryPath, args, {
                        stdio: ['pipe', 'pipe', 'pipe'],
                    });

                    if (!child.stdout || !child.stdin) {
                        reject(new Error('Failed to spawn codegraph LSP process'));
                        return;
                    }

                    this.proc = child;

                    resolve({
                        reader: child.stdout,
                        writer: child.stdin,
                    });

                    child.stderr?.on('data', (data: Buffer) => {
                        this.outputChannel.append(data.toString());
                    });

                    child.on('exit', (code) => {
                        this.outputChannel.appendLine(`exited with code ${code}`);
                        this.statusBar.setLspState('disconnected');
                        setTimeout(() => this.start(), 2000);
                    });

                    child.on('error', (err) => {
                        this.outputChannel.appendLine(`spawn error: ${err.message}`);
                    });
                });
            };

            const client = new LanguageClient(
                'ifml-lsp',
                'IFML Language Server',
                serverOptions,
                {
                    documentSelector: [{ language: 'ifml' }],
                    outputChannel: this.outputChannel,
                    diagnosticCollectionName: 'ifml',
                }
            );

            client.onDidChangeState((e) => {
                switch (e.newState) {
                    case State.Starting:
                        this.statusBar.setLspState('starting'); break;
                    case State.Running:
                        this.statusBar.setLspState('running'); break;
                    default:
                        this.statusBar.setLspState('disconnected');
                }
            });

            client.start();
        }).catch((err) => {
            this.outputChannel.appendLine(`LSP client module load failed: ${err.message}`);
            this.statusBar.setLspState('disconnected');
        });
    }

    stop(): void {
        if (this.proc) {
            this.proc.kill('SIGTERM');
            this.proc = null;
        }
    }

    async restart(): Promise<void> {
        this.stop();
        await new Promise(r => setTimeout(r, 500));
        this.start();
    }
}
