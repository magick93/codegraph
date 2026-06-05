import * as vscode from 'vscode';
import { ChildProcess, spawn } from 'child_process';

import { findBinaryPath, BinaryResult } from '../binary';

let languageClientModule: any = null;

async function getLanguageClientModule() {
  if (!languageClientModule) {
    languageClientModule = await import('vscode-languageclient/node');
  }
  return languageClientModule;
}

export class LspClient {
  private proc: ChildProcess | null = null;
  private outputChannel: vscode.OutputChannel;

  constructor(
    private context: vscode.ExtensionContext,
    private statusBar: { setLspState: (state: 'starting' | 'running' | 'disconnected') => void }
  ) {
    this.outputChannel = vscode.window.createOutputChannel('IFML Language Server');
  }

  start(): void {
    getLanguageClientModule().then(({ LanguageClient, State }) => {
      const config = vscode.workspace.getConfiguration('ifml');
      const configuredPath = config.get<string>('codegraphPath', 'codegraph');
      const schemaDirs = config.get<string[]>('schemaDirs', ['schemas']);
      const classifierPath = config.get<string>('classifierConfig', 'classifier.toml');
      const domainConfig = config.get<string>('domainConfig', 'domains.toml');

      const found = findBinaryPath(configuredPath, this.context.extensionUri.fsPath);
      const binary = found.binary;
      const binaryArgs = found.args;
      // Default cwd to workspace root so relative --schemas paths resolve
      const cwd = found.cwd || vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
      // The binary is used for LSP mode — add 'lsp' subcommand
      const args = [...binaryArgs, 'lsp'];
      for (const dir of schemaDirs) {
        args.push('--schemas', dir);
      }
      args.push('--classifier', classifierPath);
      args.push('--config', domainConfig);

      this.outputChannel.appendLine(`Starting LSP: ${binary} ${args.join(' ')}`);
      this.statusBar.setLspState('starting');

      const serverOptions = () => {
        return new Promise<{ reader: NodeJS.ReadableStream; writer: NodeJS.WritableStream }>((resolve, reject) => {
          const child: ChildProcess = spawn(binary, args, {
            stdio: ['pipe', 'pipe', 'pipe'],
            cwd: cwd,
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
            if (code !== 0) {
              this.outputChannel.appendLine('LSP server exited with error — restarting in 5s');
              setTimeout(() => this.start(), 5000);
            }
          });

          child.on('error', (err) => {
            this.outputChannel.appendLine(`spawn error: ${err.message}`);
            reject(err);
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
          initializationOptions: {
            perFileParser: { ifml: 'ifml' },
          },
        }
      );

      client.onDidChangeState((e: { newState: any }) => {
        switch (e.newState) {
          case State.Starting: this.statusBar.setLspState('starting'); break;
          case State.Running: this.statusBar.setLspState('running'); break;
          default: this.statusBar.setLspState('disconnected');
        }
      });

      client.start();
    }).catch((err) => {
      this.outputChannel.appendLine(`LSP module load failed: ${err.message}`);
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
