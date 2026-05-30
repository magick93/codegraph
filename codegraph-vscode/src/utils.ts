import * as vscode from 'vscode';

export function getDocumentText(uri: vscode.Uri): string | undefined {
    const doc = vscode.workspace.textDocuments.find(d => d.uri.toString() === uri.toString());
    return doc?.getText();
}

export function getActiveIfmlEditor(): vscode.TextEditor | undefined {
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === 'ifml') {
        return editor;
    }
    return undefined;
}

export function delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}

export function ifmlFileUris(): vscode.Uri[] {
    return vscode.workspace.textDocuments
        .filter(d => d.languageId === 'ifml')
        .map(d => d.uri);
}
