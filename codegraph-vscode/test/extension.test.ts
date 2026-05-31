import * as assert from 'assert';
import * as vscode from 'vscode';

export function run(): Promise<void> {
    const { Mocha } = require('mocha');
    const mocha = new Mocha({
        ui: 'tdd',
        timeout: 30000,
        color: true,
    });

    suite('IFML Extension Tests', function () {
        this.timeout(30000);

        test('Extension is present', () => {
            const ext = vscode.extensions.getExtension('codegraph.codegraph-ifml');
            assert.ok(ext, 'Extension should be present');
        });

        test('Extension activates', async () => {
            const ext = vscode.extensions.getExtension('codegraph.codegraph-ifml');
            assert.ok(ext);
            if (!ext?.isActive) {
                await ext?.activate();
            }
            assert.strictEqual(ext?.isActive, true);
        });

        test('Commands are registered', async () => {
            const commands = await vscode.commands.getCommands(true);
            for (const cmd of ['ifml.openDiagram', 'ifml.validate', 'ifml.generate', 'ifml.refreshLsp']) {
                assert.ok(commands.includes(cmd), `${cmd} should be registered`);
            }
        });

        test('Opens an .ifml file with correct language ID', async () => {
            const fixturesDir = vscode.Uri.file(__dirname + '/../../test/fixtures');
            const filePath = vscode.Uri.joinPath(fixturesDir, 'simple.ifml');
            const doc = await vscode.workspace.openTextDocument(filePath);
            const editor = await vscode.window.showTextDocument(doc);
            assert.strictEqual(editor.document.languageId, 'ifml');
        });
    });

    return new Promise<void>((resolve, reject) => {
        mocha.run((failures: number) => {
            if (failures > 0) {
                reject(new Error(`${failures} tests failed`));
            } else {
                resolve();
            }
        });
    });
}
