import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';

const fixtures = path.resolve(__dirname, '../../test/fixtures');

export function run(): Promise<void> {
    const Mocha = require('mocha');
    const mocha = new Mocha({ ui: 'tdd', timeout: 30000 });

    // Set up TDD globals (suite, test, setup, teardown)
    mocha.suite.emit('pre-require', global, 'extension.test', mocha);

    // Now suite/test are available as globals (cast for TypeScript)
    const globalAny = global as any;

    globalAny.suite('IFML Extension', function () {
        globalAny.test('is installed', () => {
            const ext = vscode.extensions.getExtension('codegraph.codegraph-ifml');
            assert.ok(ext, 'Extension should be present');
        });

        globalAny.test('activates on demand', async () => {
            const ext = vscode.extensions.getExtension('codegraph.codegraph-ifml');
            assert.ok(ext);
            if (!ext?.isActive) {
                await ext?.activate();
            }
            assert.strictEqual(ext?.isActive, true);
        });

        globalAny.test('registers all 4 commands', async () => {
            const cmds = await vscode.commands.getCommands(true);
            for (const cmd of ['ifml.openDiagram', 'ifml.validate', 'ifml.generate', 'ifml.refreshLsp']) {
                assert.ok(cmds.includes(cmd), `${cmd} missing`);
            }
        });

        globalAny.test('recognizes .ifml files', async () => {
            const uri = vscode.Uri.file(path.join(fixtures, 'simple.ifml'));
            const doc = await vscode.workspace.openTextDocument(uri);
            const editor = await vscode.window.showTextDocument(doc);
            assert.strictEqual(editor.document.languageId, 'ifml');
        });
    });

    return new Promise<void>((resolve, reject) => {
        mocha.run((f: number) => f > 0 ? reject(Error(`${f} failed`)) : resolve());
    });
}
