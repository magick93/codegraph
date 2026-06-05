import * as path from 'path';
import { runTests } from '@vscode/test-electron';

async function main() {
    // Test using the INSTALLED extension (from VSIX) not development path
    const extensionPath = path.resolve(process.env.HOME || '/home/anton',
        '.vscode/extensions/codegraph.codegraph-ifml-0.1.0');
    const extensionTestsPath = path.resolve(__dirname, '../out/test/extension.test');

    await runTests({
        extensionDevelopmentPath: extensionPath,
        extensionTestsPath,
        launchArgs: ['--disable-extensions'],
    });
}

main().catch(err => {
    console.error('Test failed:', err);
    process.exit(1);
});
