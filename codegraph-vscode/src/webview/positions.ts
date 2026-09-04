// Pure position-edit computation used by panel.ts as a local fallback
// when the LSP server isn't running. No vscode imports — pure string math.

export interface PositionEdit {
  start: number;
  end: number;
  newText: string;
}

function fmt(n: number): string {
  return Number(n.toFixed(2)).toString();
}

function extractBlock(text: string, start: number): { content: string; end: number } | null {
  if (text[start] !== '{') return null;
  let depth = 0;
  let i = start;
  while (i < text.length) {
    if (text[i] === '{') depth++;
    else if (text[i] === '}') {
      depth--;
      if (depth === 0) return { content: text.slice(start + 1, i), end: i + 1 };
    }
    i++;
  }
  return null;
}

const positionRe = /position\s*:\s*\{[^}]*\};?/;

function findViewBody(text: string, name: string): { bodyStart: number; body: string } | null {
  const re = new RegExp(`view\\s+"([^"]+)"\\s*\\{`, 'gs');
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    if (match[1] !== name) continue;
    const block = extractBlock(text, match.index + match[0].length - 1);
    if (block) return { bodyStart: match.index + match[0].length - 1, body: block.content };
  }
  return null;
}

function leadingIndent(text: string, offset: number): string {
  const lineStart = text.lastIndexOf('\n', offset - 1) + 1;
  const line = text.slice(lineStart, offset);
  const indentMatch = /^[ \t]*/.exec(line);
  return indentMatch ? indentMatch[0] : '';
}

function propertySectionEnd(body: string): number {
  // Position properties live in the property block before any
  // component/container/event at body level.
  const first = /(?:component\s+"|container\s+"|(?:^|\n)\s*on\s+)/.exec(body);
  return first ? first.index : body.length;
}

export function computePositionEdits(
  docText: string,
  positions: { name: string; x: number; y: number }[]
): PositionEdit[] {
  const edits: PositionEdit[] = [];
  for (const pos of positions) {
    const view = findViewBody(docText, pos.name);
    if (!view) continue;

    const newText = `position: { x: ${fmt(pos.x)}; y: ${fmt(pos.y)} };`;
    const section = view.body.slice(0, propertySectionEnd(view.body));
    const existing = positionRe.exec(section);

    if (existing) {
      edits.push({
        start: view.bodyStart + 1 + existing.index,
        end: view.bodyStart + 1 + existing.index + existing[0].length,
        newText,
      });
    } else {
      // Insert right after the opening brace of the view body.
      const indent = leadingIndent(docText, view.bodyStart) + '    ';
      edits.push({
        start: view.bodyStart + 1,
        end: view.bodyStart + 1,
        newText: `\n${indent}${newText}`,
      });
    }
  }
  return edits;
}
