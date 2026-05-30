import * as vscode from 'vscode';

const COMPONENT_TYPES = ['list', 'form', 'details', 'search', 'tree', 'chart'];
const MODE_VALUES = ['view', 'edit', 'create'];
const TYPE_REFS = ['Uuid', 'String', 'Int', 'Float', 'Boolean', 'DateTime'];
const EVENT_TYPES = ['select', 'submit', 'click', 'change', 'load', 'save', 'cancel', 'delete', 'confirm', 'back'];

export class IfmlCompletionProvider implements vscode.CompletionItemProvider {
    provideCompletionItems(
        document: vscode.TextDocument,
        position: vscode.Position,
        _token: vscode.CancellationToken,
        _context: vscode.CompletionContext
    ): vscode.CompletionItem[] {
        const linePrefix = document.lineAt(position.line).text.substring(0, position.character);
        const items: vscode.CompletionItem[] = [];

        if (linePrefix.match(/:\s*$/)) {
            for (const t of COMPONENT_TYPES) {
                items.push(new vscode.CompletionItem(t, vscode.CompletionItemKind.Value));
            }
            for (const m of MODE_VALUES) {
                items.push(new vscode.CompletionItem(m, vscode.CompletionItemKind.Value));
            }
        }

        if (linePrefix.match(/params\s*\{[^}]*$/)) {
            for (const t of TYPE_REFS) {
                items.push(new vscode.CompletionItem(t, vscode.CompletionItemKind.TypeParameter));
            }
        }

        if (linePrefix.match(/on\s+$/)) {
            for (const ev of EVENT_TYPES) {
                const item = new vscode.CompletionItem(ev, vscode.CompletionItemKind.Event);
                items.push(item);
            }
        }

        if (linePrefix.match(/navigate\s*\(\s*"([^"]*)?$/)) {
            const item = new vscode.CompletionItem('"view"', vscode.CompletionItemKind.Reference);
            item.insertText = new vscode.SnippetString('"$1"');
            items.push(item);
        }

        if (linePrefix.match(/data:\s*"?$/)) {
            const item = new vscode.CompletionItem('entity', vscode.CompletionItemKind.Class);
            item.detail = 'Entity name (loaded from JSON Schema)';
            items.push(item);
        }

        return items;
    }
}
