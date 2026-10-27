// The browser owns the live text/selection. Rust receives input for autosave and
// highlighting; only explicit document loads/reloads may replace this buffer.
export class SourceBuffer {
  constructor() { this.identity=null; this.revision=-1; }
  sync(editor, {identity,revision,text}) {
    // Ignore delayed acknowledgements and out-of-order document loads, including
    // messages whose content predates more recent browser keystrokes.
    if(revision<=this.revision)return false;
    // Textarea offsets use LF even when a Windows source file contains CRLF.
    text=text.replace(/\r\n?/g,'\n');
    const sameDocument=identity===this.identity;
    this.identity=identity;this.revision=revision;
    if(sameDocument&&editor.value===text)return false;
    const previous=editor.value;
    const selection={start:editor.selectionStart,end:editor.selectionEnd,direction:editor.selectionDirection};
    const top=editor.scrollTop,left=editor.scrollLeft;
    let prefix=0,oldEnd=previous.length,newEnd=text.length;
    if(sameDocument){
      while(prefix<oldEnd&&prefix<newEnd&&previous[prefix]===text[prefix])prefix++;
      while(oldEnd>prefix&&newEnd>prefix&&previous[oldEnd-1]===text[newEnd-1]){oldEnd--;newEnd--;}
    }
    const map=offset=>!sameDocument?0:offset<=prefix?offset:offset>=oldEnd?offset+newEnd-oldEnd:newEnd;
    editor.value=text;
    editor.setSelectionRange(map(selection.start),map(selection.end),selection.direction);
    editor.scrollTop=sameDocument?top:0;editor.scrollLeft=sameDocument?left:0;
    return true;
  }
}
