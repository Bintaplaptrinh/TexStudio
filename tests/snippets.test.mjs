import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {renderTemplate, SnippetController} from '../assets/snippets.mjs';

test('nested defaults, choices, selected text, and mirrored placeholders', () => {
  const rendered = renderTemplate('\\begin{${1:align}}\n\t${2:${TM_SELECTED_TEXT:x}}\n\\end{$1}$0', 'a+b', '  ');
  assert.equal(rendered.text, '\\begin{align}\n  \ta+b\n  \\end{align}');
  const first = rendered.stops.filter(s=>s.id===1);
  assert.equal(first.length,2);
  for (const stop of first) assert.equal(rendered.text.slice(stop.start,stop.end),'align');
  assert.equal(renderTemplate('${1|one,two|}').text,'one');
  assert.equal(renderTemplate('\\\\sqrt{$1}').text,'\\sqrt{}');
  assert.equal(renderTemplate('${1:${TM_SELECTED_TEXT:default}}').text,'default');
  assert.equal(renderTemplate('α${1:β}$0').stops[0].start,1);
});

function worker(catalog) {
  let response;
  const self = {postMessage:value=>{response=value;}};
  vm.runInNewContext(fs.readFileSync(new URL('../assets/snippet-worker.js',import.meta.url),'utf8'),{self}, {timeout:1000});
  function request(data) { self.onmessage({data:{id:1,...data}}); return response; }
  const configured = request({type:'configure',...catalog});
  assert.equal(configured.error,undefined,configured.error);
  assert.equal(configured.ready,true);
  return (text, extra={}) => request({type:'expand',text,start:text.length,end:text.length,automatic:true,...extra});
}

const liveCatalog = new URL('../target/snippet-catalog.json',import.meta.url);
test('real Workshop and HyperSnips rules, scripts, and math contexts', {skip:!fs.existsSync(liveCatalog)}, () => {
  const {catalog,enabled} = JSON.parse(fs.readFileSync(liveCatalog,'utf8').replace(/^\uFEFF/,''));
  assert.equal(typeof enabled,'boolean');
  assert.equal(catalog.snippets.length,189);
  assert.deepEqual(catalog.warnings,[]);
  const expand = worker(catalog);
  assert.equal(expand('ordinary prose sq').expansion,null);
  let result = expand('$sq');
  assert.equal(renderTemplate(result.expansion.body).text,'\\sqrt{}');
  result = expand('$a;');
  assert.equal(renderTemplate(result.expansion.body).text,'\\alpha');
  result = expand('$...');
  assert.equal(renderTemplate(result.expansion.body).text,'\\cdots ');
  result = expand('$$bmat22');
  const matrix = renderTemplate(result.expansion.body);
  assert.match(matrix.text,/\\begin\{bmatrix\}/);
  assert.match(matrix.text,/\\end\{bmatrix\}/);
  assert.equal(matrix.stops.filter(s=>s.id>0).length,4);
  assert.equal(expand('BEQ').expansion,null);
  result = expand('BEQ',{automatic:false});
  assert.match(renderTemplate(result.expansion.body).text,/\\begin\{equation\}/);
  result = expand('wordBEQ',{automatic:false});
  assert.equal(result.expansion,null);
  result = expand('\\[sq');
  assert.equal(renderTemplate(result.expansion.body).text,'\\sqrt{}');
});

test('linked fields stay synchronized as a user types', () => {
  const rendered = renderTemplate('\\begin{$1}\n\\end{$1}$0');
  const editor = {
    id:'source', value:rendered.text, selectionStart:7,selectionEnd:7,
    setRangeText(value,start,end) { this.value=this.value.slice(0,start)+value+this.value.slice(end); },
    setSelectionRange(start,end) { this.selectionStart=start;this.selectionEnd=end; },
    dispatchEvent() {},
  };
  const controller = Object.create(SnippetController.prototype);
  controller.enabled = true;
  controller.applying = false;
  controller.session = {stops:rendered.stops,ids:[1,0],index:0,previous:rendered.text};
  editor.value = '\\begin{align}\n\\end{}'; editor.selectionStart=12;editor.selectionEnd=12;
  controller.onInput({target:editor,isComposing:false});
  assert.equal(editor.value,'\\begin{align}\n\\end{align}');
  assert.equal(editor.selectionStart,12);
});
