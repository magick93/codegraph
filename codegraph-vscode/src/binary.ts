import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';

export interface BinaryResult {
  binary: string;
  args: string[];
  cwd?: string;
}

export function findBinaryPath(configured: string, extensionPath?: string): BinaryResult {
  if (fs.existsSync(configured)) {
    return { binary: configured, args: [] };
  }
  if (!configured.includes('/') && !configured.includes('\\')) {
    const paths = process.env.PATH?.split(':') || [];
    for (const p of paths) {
      const full = path.join(p, configured);
      if (fs.existsSync(full)) return { binary: full, args: [] };
    }
  }

  const searchDirs = new Set<string>();
  const addWithParents = (p: string) => {
    for (let d = p; d !== path.dirname(d); d = path.dirname(d)) {
      searchDirs.add(d);
    }
  };

  if (extensionPath) addWithParents(extensionPath);
  const workspaces = vscode.workspace.workspaceFolders;
  if (workspaces) {
    for (const ws of workspaces) addWithParents(ws.uri.fsPath);
  }
  addWithParents(process.cwd());

  for (const dir of searchDirs) {
    for (const sub of ['target/release/codegraph', 'target/debug/codegraph']) {
      const candidate = path.join(dir, sub);
      if (fs.existsSync(candidate)) return { binary: candidate, args: [] };
    }
  }

  const commonDirs = ['git/codegraph', 'codegraph', 'projects/codegraph', 'src/codegraph'];
  for (const d of Array.from(searchDirs)) {
    for (const rel of ['../', '../../', '../../../']) {
      const parent = path.resolve(d, rel);
      for (const sub of ['target/release/codegraph', 'target/debug/codegraph']) {
        const candidate = path.join(parent, sub);
        if (fs.existsSync(candidate)) return { binary: candidate, args: [] };
      }
    }
  }

  for (const d of Array.from(searchDirs)) {
    for (const sub of commonDirs) {
      for (const bin of ['target/release/codegraph', 'target/debug/codegraph']) {
        const candidate = path.join(d, sub, bin);
        if (fs.existsSync(candidate)) return { binary: candidate, args: [] };
      }
    }
  }

  for (const d of Array.from(searchDirs)) {
    const cargoPath = path.join(d, 'Cargo.toml');
    if (fs.existsSync(cargoPath)) {
      try {
        const content = fs.readFileSync(cargoPath, 'utf8');
        if (content.includes('codegraph')) {
          return { binary: 'cargo', args: ['run', '-p', 'codegraph', '--'], cwd: d };
        }
      } catch { }
    }
    for (const sub of commonDirs) {
      const subCargo = path.join(d, sub, 'Cargo.toml');
      if (fs.existsSync(subCargo)) {
        try {
          if (fs.readFileSync(subCargo, 'utf8').includes('codegraph')) {
            return { binary: 'cargo', args: ['run', '-p', 'codegraph', '--'], cwd: path.join(d, sub) };
          }
        } catch { }
      }
    }
  }
  return { binary: configured, args: [] };
}
