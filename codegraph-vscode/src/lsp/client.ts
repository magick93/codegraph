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
/// 2. target/debug/codegraph or target/release/codegraph in workspace folders
///    (walks up parent chain to find project root with Cargo.toml)
/// 3. Same but from extension install path
/// 4. Same but walking up from cwd
/// 5. cargo run as last resort — uses extension parent dir as cwd
function findBinaryPath(configured: string, extensionPath?: string): { binary: string; args: string[]; cwd?: string } {
  if (fs.existsSync(configured)) {
    return { binary: configured, args: [] };
  }
  // PATH check for simple name
  if (!configured.includes('/') && !configured.includes('\\')) {
    const paths = process.env.PATH?.split(':') || [];
    for (const p of paths) {
      const full = path.join(p, configured);
      if (fs.existsSync(full)) return { binary: full, args: [] };
    }
  }

  // Collect directories to search for target/debug/codegraph
  const searchDirs = new Set<string>();
  const addWithParents = (p: string) => { for (let d = p; d !== path.dirname(d); d = path.dirname(d)) { searchDirs.add(d); } };

  if (extensionPath) addWithParents(extensionPath);
  const workspaces = vscode.workspace.workspaceFolders;
  if (workspaces) { for (const ws of workspaces) addWithParents(ws.uri.fsPath); }
  addWithParents(process.cwd());

  for (const dir of searchDirs) {
    for (const sub of ['target/release/codegraph', 'target/debug/codegraph']) {
      const candidate = path.join(dir, sub);
      if (fs.existsSync(candidate)) return { binary: candidate, args: [] };
    }
  }

  // Fallback: search ALL search dirs for ANY Cargo.toml, try cargo from there
  // Also check common parent-relative paths for monorepo setups
  for (const d of Array.from(searchDirs)) {
    // Also check d/../ (parent) and d/../../ (grandparent) for the binary
    for (const rel of ['../', '../../', '../../../']) {
      const parent = path.resolve(d, rel);
      for (const sub of ['target/release/codegraph', 'target/debug/codegraph']) {
        const candidate = path.join(parent, sub);
        if (fs.existsSync(candidate)) return { binary: candidate, args: [] };
      }
    }
  }

  // Also search common development subdirectories of each search dir
  const commonDirs = ['git/codegraph', 'codegraph', 'projects/codegraph', 'src/codegraph'];
  for (const d of Array.from(searchDirs)) {
    for (const sub of commonDirs) {
      for (const bin of ['target/release/codegraph', 'target/debug/codegraph']) {
        const candidate = path.join(d, sub, bin);
        if (fs.existsSync(candidate)) return { binary: candidate, args: [] };
      }
    }
  }

  // Last resort: any Cargo.toml mentioning codegraph → cargo run -p codegraph
  for (const d of Array.from(searchDirs)) {
    const cargoPath = path.join(d, 'Cargo.toml');
    if (fs.existsSync(cargoPath)) {
      try {
        const content = fs.readFileSync(cargoPath, 'utf8');
        if (content.includes('codegraph')) {
          return { binary: 'cargo', args: ['run', '-p', 'codegraph', '--', 'lsp'], cwd: d };
        }
      } catch { /* ignore */ }
    }
    // Also check common subdirectories for Cargo.toml
    for (const sub of commonDirs) {
      const subCargo = path.join(d, sub, 'Cargo.toml');
      if (fs.existsSync(subCargo)) {
        try {
          if (fs.readFileSync(subCargo, 'utf8').includes('codegraph')) {
            return { binary: 'cargo', args: ['run', '-p', 'codegraph', '--', 'lsp'], cwd: path.join(d, sub) };
          }
        } catch { /* ignore */ }
      }
    }
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

      const found = findBinaryPath(configuredPath, this.context.extensionUri.fsPath);
      const binary = found.binary;
      const binaryArgs = found.args;
      const cwd = found.cwd;
      // Only add 'lsp' subcommand if not already in binaryArgs (e.g. cargo run -- lsp)
      const args = binaryArgs.includes('lsp') ? [...binaryArgs] : [...binaryArgs, 'lsp'];
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
