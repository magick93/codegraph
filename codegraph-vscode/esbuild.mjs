import * as esbuild from 'esbuild';
import { existsSync, rmSync } from 'fs';
import { join } from 'path';
import { fileURLToPath } from 'url';

// Remove stale tsc output files that would be loaded instead of the bundle
const outDir = join(fileURLToPath(new URL('.', import.meta.url)), 'out');
for (const dir of ['lsp', 'commands', 'completion', 'webview', 'status-bar.js', 'utils.js']) {
  const p = join(outDir, dir);
  if (existsSync(p)) rmSync(p, { recursive: true, force: true });
}

await esbuild.build({
  entryPoints: ['src/extension.ts'],
  bundle: true,
  outfile: 'out/extension.js',
  external: ['vscode'],
  format: 'cjs',
  platform: 'node',
  sourcemap: true,
  minify: false,
  keepNames: true,
});
