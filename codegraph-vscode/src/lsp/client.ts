import * as vscode from 'vscode';
import { ChildProcess, spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

let languageClientModule: any = null;

async function getLanguageClientModule() {
  if (!languageClientModule) {
    languageClientModule = await import('vscode-languageclient/node');
  }
  return languageClientModule;
}

/// Find the codegraph binary by checking multiple locations in order:
/// 1. User-configured path
/// 2. target/debug/codegraph or target/release/codegraph in workspace
/// 3. Same dirs but relative to extension install path (works in monorepo)
/// 4. cargo run as last resort (slow)
function findBinaryPath(configured: string, extensionPath?: string): { binary: string; args: string[] } {
  // If the configured path exists as specified, use it
  if (fs.existsSync(configured)) {
    return { binary: configured, args: [] };
  }

  // Check if it's a simple name (like "codegraph") and it's in PATH
  if (!configured.includes('/') && !configured.includes('\\')) {
    // which-like check
    const paths = process.env.PATH?.split(':') || [];
    for (const p of paths) {
      const full = path.join(p, configured);
      if (fs.existsSync(full)) {
        return { binary: full, args: [] };
      }
    }
  }

  // Collect all candidate directories to search
  const searchDirs: string[] = [];

  // From workspace folders
  const workspaces = vscode.workspace.workspaceFolders;
  if (workspaces) {
    for (const ws of workspaces) {
      searchDirs.push(ws.uri.fsPath);
      // Also check parent (for monorepo setup: workspace is codegraph-vscode/)
      const parent = path.dirname(ws.uri.fsPath);
      if (parent !== ws.uri.fsPath) searchDirs.push(parent);
    }
  }

  // From extension install path (e.g. .../codegraph.codegraph-ifml-0.1.0/)
  if (extensionPath) {
    searchDirs.push(extensionPath);
    const extParent = path.dirname(extensionPath);
    if (extParent !== extensionPath) searchDirs.push(extParent);
    // Grandparent (monorepo: ext at .../codegraph/codegraph-vscode/extensions/name)
    const grandparent = path.dirname(extParent);
    if (grandparent !== extParent) searchDirs.push(grandparent);
  }

  // Search all candidates
  for (const dir of searchDirs) {
    for (const sub of ['target/debug/codegraph', 'target/release/codegraph']) {
      const candidate = path.join(dir, sub);
      if (fs.existsSync(candidate)) {
        return { binary: candidate, args: [] };
      }
    }
  }

  // Fallback: use cargo run (triggers build — slow first time)
  if (workspaces && workspaces.length > 0) {
    return { binary: 'cargo', args: ['run', '--', 'lsp'] };
  }

  return { binary: configured, args: [] };
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

      const { binary, args: binaryArgs } = findBinaryPath(configuredPath, this.context.extensionUri.fsPath);
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
            cwd: binary === 'cargo' ? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath : undefined,
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
