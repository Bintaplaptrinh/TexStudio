import test from 'node:test';
import assert from 'node:assert/strict';
import {SourceBuffer} from '../assets/source-buffer.mjs';

function textarea(){
  let text='';
  return {writes:0,selectionStart:0,selectionEnd:0,selectionDirection:'none',scrollTop:0,scrollLeft:0,
    get value(){return text;},set value(value){text=value;this.writes++;this.selectionStart=this.selectionEnd=value.length;},
    setSelectionRange(start,end,direction='none'){this.selectionStart=start;this.selectionEnd=end;this.selectionDirection=direction;}
  };
}

test('late Rust input echoes never replace newer typing or move the caret to end of document',()=>{
  const buffer=new SourceBuffer(),editor=textarea();
  const initial='\\begin{\nsecond line\n\\end{document}';
  buffer.sync(editor,{identity:'paper:main.tex',revision:1,text:initial});
  // A real textarea moves the selection to the end whenever a different value
  // is assigned. Emulate Enter and continued typing before the document end.
  editor.value='\\begin{\ncursor-marker\nsecond line\n\\end{document}';
  const caret=editor.value.indexOf('cursor-marker')+'cursor-marker'.length;
  editor.setSelectionRange(caret,caret);editor.scrollTop=84;
  const writes=editor.writes;
  for(const text of [initial,'\\begin{\nc','\\begin{\ncursor']){
    assert.equal(buffer.sync(editor,{identity:'paper:main.tex',revision:1,text}),false);
  }
  assert.equal(editor.writes,writes);
  assert.equal(editor.selectionStart,caret);
  assert.equal(editor.scrollTop,84);
  assert.match(editor.value,/cursor-marker\nsecond line\n\\end\{document\}$/);
});

test('file switches load once and reject older asynchronous loads',()=>{
  const buffer=new SourceBuffer(),editor=textarea();
  buffer.sync(editor,{identity:'a.tex',revision:1,text:'First document'});
  buffer.sync(editor,{identity:'b.tex',revision:3,text:'Second document'});
  assert.equal(editor.selectionStart,0);
  assert.equal(buffer.sync(editor,{identity:'a.tex',revision:2,text:'Old pending load'}),false);
  assert.equal(editor.value,'Second document');
});

test('explicit disk reload maps selections and preserves scroll instead of jumping to EOF',()=>{
  const buffer=new SourceBuffer(),editor=textarea();
  buffer.sync(editor,{identity:'main.tex',revision:1,text:'header\nbody\n\\end{document}'});
  editor.setSelectionRange(7,11,'backward');editor.scrollTop=120;editor.scrollLeft=18;
  buffer.sync(editor,{identity:'main.tex',revision:2,text:'new header\nbody\n\\end{document}'});
  assert.equal(editor.value.slice(editor.selectionStart,editor.selectionEnd),'body');
  assert.equal(editor.selectionDirection,'backward');
  assert.equal(editor.scrollTop,120);assert.equal(editor.scrollLeft,18);
});

test('Windows line endings do not turn an unchanged reload into a full replacement',()=>{
  const buffer=new SourceBuffer(),editor=textarea();
  const text='first line\r\nsecond line\r\n\\end{document}';
  buffer.sync(editor,{identity:'windows.tex',revision:1,text});
  editor.setSelectionRange(12,15);editor.scrollTop=90;
  const writes=editor.writes;
  assert.equal(buffer.sync(editor,{identity:'windows.tex',revision:2,text}),false);
  assert.equal(editor.writes,writes);assert.equal(editor.selectionStart,12);assert.equal(editor.scrollTop,90);
});
