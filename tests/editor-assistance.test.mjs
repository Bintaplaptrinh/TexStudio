import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {latexContext,completionQuery,createCatalog,suggestions,autoClosingEdit,closeEnvironmentEdit,completionEdit} from '../assets/latex-model.mjs';
import {renderTemplate,SnippetController} from '../assets/snippets.mjs';
import {EditorAssistance} from '../assets/editor-assistance.mjs';

const data=Object.fromEntries(['aerijo','macros','environments','packagenames','classnames'].map(name=>[name,JSON.parse(fs.readFileSync(new URL(`../assets/completions/${name}.json`,import.meta.url),'utf8'))]));
const catalog=createCatalog(data);
function suggest(text,project={},manual=false){return suggestions(text,text.length,catalog,project,'main.tex',manual);}
function apply(text,edit){return edit ? {text:text.slice(0,edit.start)+edit.text+text.slice(edit.end),caret:edit.caret} : {text,caret:text.length};}
function completion(text,label,project={}){
  const {query,items}=suggest(text,project);
  const item=items.find(item=>item.label===label);assert.ok(item,`Missing ${label}`);
  const edit=completionEdit(text,query,item,true),rendered=renderTemplate(edit.body);
  return text.slice(0,edit.start)+rendered.text+text.slice(edit.end);
}

test('completion list uses Aerijo data, fuzzy matching, and math scope',()=>{
  assert.ok(catalog.commands.length>300);
  assert.equal(suggest('ordinary prose').items.length,0);
  assert.equal(suggest('\\fr').items.some(s=>s.label==='\\frac'),false);
  assert.equal(suggest('$\\fr').items[0].label,'\\frac');
  assert.equal(completion('$\\fr','\\frac'),'$\\frac{}{}');
  assert.ok(suggest('\\sbb').items.some(s=>s.label==='\\subsubsection'));
  assert.ok(suggest('\\beg').items.some(s=>s.label==='\\begin'));
  assert.ok(suggest('\\').items.length>0);
  assert.equal(suggest('\\\\').items.length,0);
  assert.ok(suggest('\\',{},true).items.length>0);
  assert.equal(suggest('% comment \\sec').items.length,0);
  assert.equal(suggest('\\begin{verbatim}\n\\sec').items.length,0);
  assert.equal(latexContext('\\verb|$x|').math,false);
  assert.equal(latexContext('\\$10').math,false);
  assert.equal(latexContext('\\[x').math,true);
  assert.equal(latexContext('\\begin{align}x').math,true);
  assert.equal(latexContext('\\begin{align}x\\end{align}').math,false);
});

test('packages, classes, references, citations, and files use argument contexts',()=>{
  const index={labels:[{name:'sec:method',path:'sections/method.tex'}],citations:[{name:'doe2026',path:'references.bib'}],files:['main.tex','sections/intro.tex','figures/result.png','references.bib']};
  assert.ok(suggest('\\usepackage{ams').items.some(s=>s.label==='amsmath'));
  assert.ok(suggest('\\documentclass{art').items.some(s=>s.label==='article'));
  assert.equal(suggest('\\ref{sec:',index).items[0].label,'sec:method');
  assert.equal(suggest('\\cite[see]{doe',index).items[0].label,'doe2026');
  assert.equal(completion('Read @doe','doe2026',index),'Read \\cite{doe2026}');
  assert.equal(suggest('mail@example').items.length,0);
  assert.equal(completion('\\cite{old, doe','doe2026',index),'\\cite{old, doe2026');
  assert.equal(suggest('\\includegraphics{fig',index).items[0].label,'figures/result.png');
  assert.equal(suggest('\\input{sec',index).items[0].label,'sections/intro.tex');
  assert.equal(suggest('\\addbibresource{ref',index).items[0].label,'references.bib');
});

test('custom definitions and labels work before autosave and update with project data',()=>{
  const text='\\newcommand{\\myvector}[2]{#1+#2}\n\\label{sec:unsaved}\n\\myv';
  const result=suggest(text);
  assert.equal(result.items[0].label,'\\myvector');
  assert.equal(completion(text,'\\myvector'),text.slice(0,-4)+'\\myvector{}{}');
  assert.equal(suggest('\\label{new}\n\\ref{n').items[0].label,'new');
  assert.equal(suggest('\\newenvironment{claim}{}{}\n\\begin{cl').items[0].label,'claim');
  const index={commands:[{name:'external',args:1,path:'macros.tex'}]};
  assert.equal(suggest('\\ext',index).items[0].label,'\\external');
});

test('environment completion and typing do not duplicate existing end tags',()=>{
  assert.equal(completion('\\begin{equ','equation'),'\\begin{equation}\\end{equation}');
  const source='\\begin{equ}\ntext\n\\end{equation}';
  const result=suggestions(source,10,catalog,{},'main.tex');
  const chosen=result.items.find(s=>s.label==='equation');assert.ok(chosen);
  const edit=completionEdit(source,result.query,chosen,true);
  assert.equal(source.slice(0,edit.start)+renderTemplate(edit.body).text+source.slice(edit.end),'\\begin{equation}\ntext\n\\end{equation}');
  assert.equal(closeEnvironmentEdit('\\begin{align}x\\end{align}',13),null);
  const nested='\\begin{document}\n\\begin{align}x\\end{document}';
  assert.ok(closeEnvironmentEdit(nested,nested.indexOf('}x')+1));
  assert.equal(closeEnvironmentEdit('% \\begin{figure}',16),null);
  assert.equal(closeEnvironmentEdit('\\begin{verbatim}\\begin{figure}',30),null);
  const unfinished='\\begin{align';
  const finished=apply(unfinished,autoClosingEdit(unfinished,unfinished.length,unfinished.length,'}'));
  assert.deepEqual(finished,{text:'\\begin{align}\\end{align}',caret:13});
  const entered=apply('  \\begin{align}\\end{align}',autoClosingEdit('  \\begin{align}\\end{align}',15,15,'Enter'));
  assert.equal(entered.text,'  \\begin{align}\n    \n  \\end{align}');
  assert.equal(entered.caret,20);
});

test('pairs wrap selections, skip closers, erase empty pairs, and respect comments and escapes',()=>{
  assert.equal(apply('word',autoClosingEdit('word',0,4,'{')).text,'{word}');
  assert.equal(apply('',autoClosingEdit('',0,0,'{')).text,'{}');
  const skipped=apply('{}',autoClosingEdit('{}',1,1,'}'));
  assert.deepEqual(skipped,{text:'{}',caret:2});
  assert.equal(apply('{}',autoClosingEdit('{}',1,1,'Backspace')).text,'');
  assert.equal(apply('$x$',autoClosingEdit('$x$',2,2,'$')).text,'$x$');
  assert.deepEqual(apply('$$',autoClosingEdit('$$',1,1,'$')),{text:'$$$$',caret:2});
  assert.equal(apply('\\',autoClosingEdit('\\',1,1,'(')).text,'\\(\\)');
  assert.equal(apply('\\left',autoClosingEdit('\\left',5,5,'(')).text,'\\left(\\right)');
  assert.equal(autoClosingEdit('% comment',9,9,'{'),null);
  assert.equal(autoClosingEdit('\\begin{verbatim}\n',17,17,'{'),null);
  assert.equal(autoClosingEdit('\\',1,1,'$'),null);
  const slash=apply('\\(x\\)',autoClosingEdit('\\(x\\)',3,3,'\\'));
  assert.equal(slash.text,'\\(x\\)');
  assert.deepEqual(apply(slash.text,autoClosingEdit(slash.text,slash.caret,slash.caret,')')),{text:'\\(x\\)',caret:5});
});

test('completion placeholders can be navigated with optional snippets off',()=>{
  const controller=Object.create(SnippetController.prototype);
  controller.enabled=false;controller.session={ids:[1,0],index:0};
  let selected=false;controller.selectStop=()=>{selected=true;};
  let prevented=false;
  assert.equal(controller.onKeydown({target:{id:'source'},key:'Tab',shiftKey:false,preventDefault(){prevented=true;}}),true);
  assert.ok(selected&&prevented);
});

test('popup keys take precedence over indentation and placeholder navigation',()=>{
  const controller=Object.create(EditorAssistance.prototype);
  controller.config={editable:true,autocomplete:true,autoClosing:true};controller.popup={hidden:false};
  controller.result={items:[{label:'one'},{label:'two'}]};controller.selected=0;
  let highlighted=0,accepted=0,hidden=0,prevented=0;
  controller.highlight=()=>highlighted++;
  controller.accept=()=>accepted++;
  controller.hide=()=>hidden++;
  const event=key=>({target:{id:'source'},key,preventDefault(){prevented++},stopImmediatePropagation(){}});
  controller.onKeydown(event('ArrowDown'));
  assert.equal(controller.selected,1);assert.equal(highlighted,1);
  controller.onKeydown(event('Enter'));assert.equal(accepted,1);
  controller.onKeydown(event('Tab'));assert.equal(accepted,2);
  controller.onKeydown(event('Escape'));assert.equal(hidden,1);
  assert.equal(prevented,4);
});

test('environment closing also handles paste and replacement input events',()=>{
  const controller=Object.create(EditorAssistance.prototype);
  controller.config={editable:true,autoClosing:true};
  let applied;
  controller.apply=edit=>{applied=edit;};controller.schedule=()=>{};
  const text='\\begin{align}';
  controller.onInput({target:{id:'source',value:text,selectionStart:text.length,selectionEnd:text.length},inputType:'insertFromPaste',data:null});
  assert.equal(applied.text,'\\end{align}');
});

test('Enter after begin brace starts a new line when a completion was not explicitly selected',()=>{
  const controller=Object.create(EditorAssistance.prototype);
  controller.config={editable:true,autocomplete:true,autoClosing:false};controller.popup={hidden:false};
  controller.result={items:[{label:'document'}]};controller.selected=0;controller.selectionExplicit=false;
  let accepted=false,hidden=false,prevented=false;
  controller.accept=()=>{accepted=true;};controller.hide=()=>{hidden=true;};
  controller.onKeydown({target:{id:'source',value:'\\begin{',selectionStart:7,selectionEnd:7},key:'Enter',preventDefault(){prevented=true;},stopImmediatePropagation(){}});
  assert.equal(accepted,false);assert.equal(prevented,false);assert.equal(hidden,true);
});
