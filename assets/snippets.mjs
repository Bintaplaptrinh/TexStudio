// VS Code-style placeholders, with offsets measured in textarea UTF-16 units.
export function renderTemplate(template, selected = '', indent = '') {
  const values = new Map();
  const stops = [];
  let output = '';
  function parse(input) {
    let i = 0;
    while (i < input.length) {
      if (input[i] === '\\' && /[\\$}]/.test(input[i + 1] || '')) { output += input[i + 1]; i += 2; continue; }
      if (input[i] !== '$') { output += input[i++]; continue; }
      const numeric = /^\$(\d+)/.exec(input.slice(i));
      if (numeric) {
        const id = Number(numeric[1]), start = output.length;
        output += values.get(id) || '';
        stops.push({id, start, end:output.length}); i += numeric[0].length; continue;
      }
      if (input[i + 1] !== '{') { output += input[i++]; continue; }
      let end = i + 2, depth = 1;
      for (; end < input.length; end++) {
        if (input[end] === '\\') { end++; continue; }
        if (input[end] === '{') depth++;
        if (input[end] === '}' && --depth === 0) break;
      }
      if (end >= input.length) { output += input[i++]; continue; }
      const inner = input.slice(i + 2, end);
      const numbered = /^(\d+)(?::([\s\S]*)|\|([\s\S]*)\|)?$/.exec(inner);
      if (numbered) {
        const id = Number(numbered[1]), start = output.length;
        if (values.has(id)) output += values.get(id);
        else {
          parse(numbered[2] ?? numbered[3]?.split(',')[0] ?? '');
          values.set(id, output.slice(start));
        }
        stops.push({id, start, end:output.length});
      } else {
        const variable = /^([A-Z_]+)(?::([\s\S]*))?$/.exec(inner);
        if (variable?.[1] === 'TM_SELECTED_TEXT') output += selected || variable[2] || '';
        else if (variable?.[2]) parse(variable[2]);
        else output += '${' + inner + '}';
      }
      i = end + 1;
    }
  }
  parse(template);
  // Every generated line inherits the indentation at the insertion point.
  if (indent) {
    const unindented = output;
    const adjustment = offset => (unindented.slice(0, offset).match(/\n/g) || []).length * indent.length;
    for (const stop of stops) { stop.end += adjustment(stop.end); stop.start += adjustment(stop.start); }
    output = output.replace(/\n/g, '\n' + indent);
  }
  if (!stops.some(s => s.id === 0)) stops.push({id:0, start:output.length, end:output.length});
  stops.sort((a,b) => (a.id === 0 ? Infinity : a.id) - (b.id === 0 ? Infinity : b.id) || a.start - b.start);
  return {text:output, stops};
}

export class SnippetController {
  constructor(workerSource) {
    this.workerSource = workerSource;
    this.sequence = 0;
    this.pending = new Map();
    this.enabled = false;
    this.session = null;
    this.applying = false;
    this.identity = '';
    document.addEventListener('input', event => this.onInput(event));
    window.texstudioSnippetKeydown = event => this.onKeydown(event);
    window.texstudioInsertSnippet = id => this.expand(false, id);
  }
  message(text) {
    const hint = document.getElementById('snippet-hint');
    if (hint) hint.textContent = text;
  }
  restart() {
    this.worker?.terminate();
    for (const pending of this.pending.values()) { clearTimeout(pending.timeout); pending.resolve({expansion:null}); }
    this.pending.clear();
    const url = URL.createObjectURL(new Blob([this.workerSource], {type:'text/javascript'}));
    this.worker = new Worker(url);
    URL.revokeObjectURL(url);
    this.worker.onmessage = event => {
      const pending = this.pending.get(event.data.id);
      if (!pending) return;
      clearTimeout(pending.timeout); this.pending.delete(event.data.id); pending.resolve(event.data);
    };
    this.worker.onerror = event => { this.message(`Snippet error: ${event.message}`); this.worker.terminate(); };
  }
  async configure(config) {
    this.enabled = config.enabled;
    this.session = null;
    this.identity = config.identity;
    this.catalog = config.catalog;
    if (!this.enabled) {
      this.worker?.terminate(); this.worker = null;
      for (const pending of this.pending.values()) { clearTimeout(pending.timeout); pending.resolve({expansion:null}); }
      this.pending.clear();
      return;
    }
    this.restart();
    const response = await this.request({type:'configure', ...this.catalog}, 2000);
    this.message(response.error ? `Snippet error: ${response.error}` : 'Tab to expand · Tab / Shift+Tab through fields');
  }
  request(data, milliseconds = 180) {
    if (!this.worker) return Promise.resolve({expansion:null});
    const id = ++this.sequence;
    return new Promise(resolve => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        this.worker?.terminate(); this.worker = null;
        this.message('Snippet took too long. Toggle snippets in Settings to restart.');
        resolve({expansion:null, error:'Snippet timed out'});
      }, milliseconds);
      this.pending.set(id, {resolve, timeout}); this.worker.postMessage({...data,id});
    });
  }
  async expand(automatic, snippetId) {
    if (!this.enabled || !this.worker) return false;
    const editor = document.getElementById('source');
    if (!editor) return false;
    const original = editor.value, start = editor.selectionStart, end = editor.selectionEnd;
    const identity = this.identity;
    const result = await this.request({type:'expand', text:original, start, end, automatic, snippetId});
    if (result.error) { this.message(result.error); return false; }
    if (identity !== this.identity || editor.value !== original || editor.selectionStart !== start || editor.selectionEnd !== end) return false;
    const expansion = result.expansion;
    if (!expansion) return false;
    this.insertTemplate(expansion);
    return true;
  }
  insertTemplate(expansion) {
    const editor = document.getElementById('source');
    if (!editor) return false;
    const original = editor.value;
    const indent = /^\s*/.exec(original.slice(0, expansion.start).split('\n').at(-1))[0];
    const rendered = renderTemplate(expansion.body, expansion.selected, indent);
    this.applying = true;
    editor.focus(); editor.setSelectionRange(expansion.start,expansion.end);
    const inserted = document.execCommand?.('insertText',false,rendered.text);
    if (!inserted) editor.setRangeText(rendered.text, expansion.start, expansion.end, 'end');
    const stops = rendered.stops.map(s => ({...s,start:s.start + expansion.start,end:s.end + expansion.start}));
    const ids = [...new Set(stops.map(s => s.id))];
    this.session = {stops, ids, index:0, previous:editor.value};
    this.selectStop(editor);
    if (!inserted) editor.dispatchEvent(new Event('input',{bubbles:true}));
    this.applying = false;
    this.message('Tab / Shift+Tab to move between fields · Esc to finish');
    return true;
  }
  selectStop(editor) {
    const session = this.session;
    const id = session.ids[session.index];
    const stop = session.stops.find(s => s.id === id);
    if (!stop) { this.session = null; return; }
    editor.focus(); editor.setSelectionRange(stop.start,stop.end);
    if (id === 0) this.session = null;
  }
  onInput(event) {
    if (event.target.id !== 'source' || this.applying || (!this.enabled && !this.session) || event.isComposing) return;
    const editor = event.target;
    if (this.session) {
      const session = this.session;
      const old = session.previous, next = editor.value;
      let start = 0;
      while (start < old.length && start < next.length && old[start] === next[start]) start++;
      let a = old.length, b = next.length;
      while (a > start && b > start && old[a-1] === next[b-1]) { a--; b--; }
      const id = session.ids[session.index];
      const active = session.stops.find(s => s.id === id);
      if (start < active.start || a > active.end) { this.session = null; }
      else {
        const delta = b - a;
        const primary = active;
        for (const stop of session.stops) {
          if (stop === primary) stop.end += delta;
          else if (stop.start >= a) { stop.start += delta; stop.end += delta; }
          else if (stop.end >= a) stop.end += delta;
        }
        const value = next.slice(primary.start, primary.end);
        let selectionStart = editor.selectionStart, selectionEnd = editor.selectionEnd;
        this.applying = true;
        for (const mirror of session.stops.filter(s=>s.id === id && s !== primary).sort((a,b)=>b.start-a.start)) {
          const delta = value.length - (mirror.end - mirror.start), at = mirror.start, finish = mirror.end;
          editor.setRangeText(value,at,finish,'preserve');
          for (const stop of session.stops) {
            if (stop === mirror) stop.end = stop.start + value.length;
            else if (stop.start >= finish) { stop.start += delta; stop.end += delta; }
            else if (stop.end >= finish) stop.end += delta;
          }
          if (selectionStart >= finish) { selectionStart += delta; selectionEnd += delta; }
        }
        editor.setSelectionRange(selectionStart,selectionEnd);
        session.previous = editor.value;
        if (editor.value !== next) editor.dispatchEvent(new Event('input',{bubbles:true}));
        this.applying = false;
        return;
      }
    }
    if (this.enabled && !globalThis.window?.texstudioApplyingEdit) this.expand(true);
  }
  onKeydown(event) {
    if (event.target.id !== 'source' || (!this.enabled && !this.session)) return false;
    if (event.key === 'Escape') { this.session = null; return false; }
    if (event.key !== 'Tab') return false;
    event.preventDefault();
    const editor = event.target;
    if (this.session) {
      this.session.index = Math.max(0,this.session.index + (event.shiftKey ? -1 : 1));
      if (this.session.index >= this.session.ids.length) this.session = null;
      else this.selectStop(editor);
    } else if (!event.shiftKey) {
      this.expand(false).then(expanded => {
        if (!expanded && document.activeElement === editor) {
          editor.setRangeText('  ',editor.selectionStart,editor.selectionEnd,'end');
          editor.dispatchEvent(new Event('input',{bubbles:true}));
        }
      });
    }
    return true;
  }
}
