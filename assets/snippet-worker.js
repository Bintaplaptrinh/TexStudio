// HyperSnips runs here, without a DOM, Node.js, filesystem, or VS Code host.
// Source comes from the local .hsnips file explicitly selected in settings.
self.fetch = undefined;
self.XMLHttpRequest = undefined;
self.WebSocket = undefined;
self.importScripts = undefined;
let snippets = [];
let runtime;
let text = '';
let selected = '';
let caret = 0;
const selectionListeners = [];

class Position { constructor(line, character) { this.line = line; this.character = character; } }
class Range { constructor(start, end) { this.start = start; this.end = end; } }
function offset(position) {
  const lines = text.split('\n');
  return lines.slice(0, position.line).reduce((n, line) => n + line.length + 1, 0) + position.character;
}
function position(index) {
  const lines = text.slice(0, index).split('\n');
  return new Position(lines.length - 1, lines.at(-1).length);
}
const doc = {
  getText(range) { return range ? text.slice(offset(range.start), offset(range.end)) : text; },
  lineAt(pos) {
    const line = typeof pos === 'number' ? pos : pos.line;
    const value = text.split('\n')[line] || '';
    return {text: value, range: new Range(new Position(line, 0), new Position(line, value.length))};
  },
};
const editor = {document: doc, selection: {active: new Position(0, 0), start: new Position(0, 0)}, edit: () => Promise.resolve(false)};
const vscode = {
  Position, Range,
  window: {
    activeTextEditor: editor,
    onDidChangeActiveTextEditor() {},
    onDidChangeTextEditorSelection(callback) { selectionListeners.push(callback); },
    setStatusBarMessage() {},
    // Package suggestions never modify a document implicitly.
    showInformationMessage: () => Promise.resolve(undefined),
  },
};

function mathScopes(source) {
  let inline = false, display = false;
  const environments = [];
  const mathEnv = /^(?:equation|align|alignat|gather|multline|flalign|math|displaymath|eqnarray|split|cases|matrix|[pbBvV]matrix)\*?$/;
  const clean = source.replace(/\\verb\*?([^\w\s])[^\n]*?\1/g, '').replace(/(?<!\\)%[^\n]*/g, '');
  const tokens = /\\begin\{([^}]+)\}|\\end\{([^}]+)\}|\\[()[\]]|(?<!\\)\$\$?/g;
  for (const match of clean.matchAll(tokens)) {
    if (match[1]) environments.push(match[1]);
    else if (match[2]) { const i = environments.lastIndexOf(match[2]); if (i >= 0) environments.splice(i); }
    else if (match[0] === '$$') display = !display;
    else if (match[0] === '$') inline = !inline;
    else if (match[0] === '\\[') display = true;
    else if (match[0] === '\\]') display = false;
    else if (match[0] === '\\(') inline = true;
    else if (match[0] === '\\)') inline = false;
  }
  if (environments.some(e => /^(?:verbatim|lstlisting|minted)\*?$/.test(e))) return [];
  if (display || environments.some(e => mathEnv.test(e))) return ['math', 'math.block.environment'];
  return inline ? ['math', 'math.inline'] : [];
}

function evaluateBody(body, match) {
  let output = '';
  let i = 0;
  while (i < body.length) {
    const start = body.indexOf('``', i);
    if (start < 0) { output += body.slice(i); break; }
    output += body.slice(i, start);
    let end = start + 2, quote = '', escaped = false;
    for (; end < body.length; end++) {
      const c = body[end];
      if (quote) {
        if (c === quote && !escaped) quote = '';
        escaped = c === '\\' && !escaped;
      } else {
        if (body.startsWith('``', end)) break;
        if (c === '"' || c === "'" || c === '`') quote = c;
      }
    }
    if (end >= body.length) throw new Error('Unclosed HyperSnips expression');
    output += String(runtime.render(body.slice(start + 2, end), match, {tabstop: n => `$${n}`}));
    i = end + 2;
  }
  return output;
}

self.onmessage = event => {
  const request = event.data;
  try {
    if (request.type === 'configure') {
      selectionListeners.length = 0;
      runtime = new Function('require', 'setInterval', `${request.global || ''}\nreturn {
        render(code, m, snip) { let rv = ''; eval(code); return rv; },
        test(expression, context) { return !expression || !!eval(expression); }
      };`)(name => { if (name !== 'vscode') throw new Error('Unsupported module'); return vscode; }, () => 0);
      snippets = request.snippets.map(s => ({...s, pattern: s.regex ? new RegExp(`(?:${s.trigger})$`) : null}))
        .sort((a,b) => b.priority - a.priority || b.trigger.length - a.trigger.length);
      self.postMessage({id: request.id, ready: true}); return;
    }
    text = request.text;
    caret = request.end;
    selected = text.slice(request.start, request.end);
    const active = position(caret);
    editor.selection = {start: position(request.start), end: active, active, isEmpty: !selected};
    for (const callback of selectionListeners) callback({textEditor: editor, selections: [editor.selection]});
    const before = text.slice(0, request.start);
    const context = {scopes: mathScopes(before)};
    for (const snippet of snippets) {
      if (request.snippetId && snippet.id !== request.snippetId) continue;
      if (request.automatic && !snippet.automatic) continue;
      let match = snippet.regex ? snippet.pattern.exec(before) : before.endsWith(snippet.trigger) ? Object.assign([snippet.trigger], {index: before.length - snippet.trigger.length}) : null;
      if (!match && !request.snippetId) continue;
      const at = match ? match.index : request.start;
      if (!request.snippetId && snippet.line_start && before.slice(0,at).split('\n').at(-1).trim()) continue;
      if (!request.snippetId && !snippet.in_word && !snippet.regex && /[\w]/u.test(before[at - 1] || '')) continue;
      if (!request.snippetId && !runtime.test(snippet.context, context)) continue;
      if (!match) match = [];
      try {
        const body = snippet.source === 'HyperSnips' ? evaluateBody(snippet.body, match) : snippet.body;
        self.postMessage({id:request.id, expansion:{id:snippet.id, body, source:snippet.source, start:at, end:request.end, selected}});
        return;
      } catch (error) {
        if (request.snippetId) throw new Error('Type the snippet trigger in the editor first. ' + error.message);
      }
    }
    self.postMessage({id:request.id, expansion:null});
  } catch (error) { self.postMessage({id:request.id, error:error.message}); }
};
