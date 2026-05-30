import * as vscode from 'vscode';
import {
    LanguageClient, LanguageClientOptions, ServerOptions, StreamInfo, State
} from 'vscode-languageclient/node';
import { ChildProcess, spawn } from 'child_process';
import { StatusBar } from '../status-bar';

export class LspClient {
    private client: LanguageClient | null = null;
    private proc: ChildProcess | null = null;
    private outputChannel: vscode.OutputChannel;

    constructor(
        private context: vscode.ExtensionContext,
        private statusBar: StatusBar
    ) {
        this.outputChannel = vscode.window.createOutputChannel('IFML Language Server');
    }

    start(): void {
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

        const serverOptions: ServerOptions = () => {
            return new Promise<StreamInfo>((resolve, reject) => {
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

        const clientOptions: LanguageClientOptions = {
            documentSelector: [{ language: 'ifml' }],
            synchronize: {
                configurationSection: 'ifml',
                fileEvents: vscode.workspace.createFileSystemWatcher('**/*.ifml'),
            },
            outputChannel: this.outputChannel,
            diagnosticCollectionName: 'ifml',
        };

        this.client = new LanguageClient(
            'ifml-lsp',
            'IFML Language Server',
            serverOptions,
            clientOptions
        );

        this.client.onDidChangeState((e) => {
            const s = e.newState;
            if (s === State.Starting) {
                this.statusBar.setLspState('starting');
            } else if (s === State.Running) {
                this.statusBar.setLspState('running');
            } else {
                this.statusBar.setLspState('disconnected');
            }
        });

        this.client.start();
    }

    stop(): void {
        if (this.client) {
            this.client.stop();
        }
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

    getClient(): LanguageClient | null {
        return this.client;
    }
}
