import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { computePositionEdits } from '../src/webview/positions';
import { parseIfmlForDiagram } from '../src/webview/parser';

const fixtures = path.resolve(__dirname, '../../../test/fixtures');

function applyEdits(text: string, edits: { start: number; end: number; newText: string }[]): string {
    let result = text;
    // Apply from the end so offsets stay valid
    const sorted = [...edits].sort((a, b) => b.start - a.start);
    for (const e of sorted) {
        result = result.slice(0, e.start) + e.newText + result.slice(e.end);
    }
    return result;
}

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

    globalAny.suite('computePositionEdits (diagram → text fallback)', function () {
        const fixture = fs.readFileSync(path.join(fixtures, 'full.ifml'), 'utf8');

        globalAny.test('inserts a position property for a view without one', () => {
            const edits = computePositionEdits(fixture, [{ name: 'CustomerList', x: 120, y: 240 }]);
            assert.strictEqual(edits.length, 1);
            assert.ok(edits[0].newText.includes('position: { x: 120; y: 240 };'), `unexpected newText: ${edits[0].newText}`);
            assert.strictEqual(edits[0].start, edits[0].end);

            const updated = applyEdits(fixture, edits);
            const model = parseIfmlForDiagram(updated);
            const vc = model.viewContainers.find(v => v.name === 'CustomerList');
            assert.ok(vc, 'CustomerList still present');
            assert.deepStrictEqual(vc?.position, { x: 120, y: 240 });
            // exactly one position property in the whole doc
            assert.strictEqual((updated.match(/position\s*:/g) || []).length, 1);
        });

        globalAny.test('replaces an existing position property in place', () => {
            const withPos = applyEdits(fixture, computePositionEdits(fixture, [{ name: 'CustomerList', x: 1, y: 2 }]));
            assert.strictEqual((withPos.match(/position\s*:/g) || []).length, 1);

            const edits = computePositionEdits(withPos, [{ name: 'CustomerList', x: 300, y: 400 }]);
            assert.strictEqual(edits.length, 1);
            assert.ok(edits[0].start < edits[0].end, 'replacement should not be a pure insertion');

            const updated = applyEdits(withPos, edits);
            const model = parseIfmlForDiagram(updated);
            const vc = model.viewContainers.find(v => v.name === 'CustomerList');
            assert.deepStrictEqual(vc?.position, { x: 300, y: 400 });
            assert.strictEqual((updated.match(/position\s*:/g) || []).length, 1);
        });

        globalAny.test('unknown view names are skipped', () => {
            const edits = computePositionEdits(fixture, [{ name: 'NoSuchView', x: 1, y: 2 }]);
            assert.strictEqual(edits.length, 0);
        });
    });

    return new Promise<void>((resolve, reject) => {
        mocha.run((f: number) => f > 0 ? reject(Error(`${f} failed`)) : resolve());
    });
}
