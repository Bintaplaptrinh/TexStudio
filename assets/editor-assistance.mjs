import {suggestions,completionEdit,autoClosingEdit,closeEnvironmentEdit} from './latex-model.mjs';

export class EditorAssistance {
  constructor(catalog, snippets) {
    this.catalog=catalog; this.snippets=snippets;
    this.config={editable:false,autocomplete:false,autoClosing:false,index:{}};
    this.result={items:[],query:null}; this.selected=0; this.applying=false;
    this.popup=document.createElement('div');
    this.popup.id='completion-widget'; this.popup.className='completion-widget'; this.popup.hidden=true;
    this.popup.setAttribute('role','presentation');
    this.list=document.createElement('div'); this.list.id='completion-list'; this.list.className='completion-list'; this.list.setAttribute('role','listbox'); this.list.setAttribute('aria-label','LaTeX completions');
    this.detail=document.createElement('div'); this.detail.className='completion-detail';
    this.help=document.createElement('div'); this.help.className='completion-help'; this.help.textContent='Tab insert   ↑ ↓ then Enter select   Esc close';
    this.popup.append(this.list,this.detail,this.help); document.body.append(this.popup);
    document.addEventListener('keydown',event=>this.onKeydown(event),true);
    // Finalize auto-closing before the framework serializes the textarea value;
    // otherwise an intermediate value can move the caret to the buffer's end.
    document.addEventListener('input',event=>this.onInput(event),true);
    document.addEventListener('compositionstart',event=>{if(event.target.id==='source'){this.composing=true;this.hide();}});
    document.addEventListener('compositionend',event=>{if(event.target.id==='source'){this.composing=false;this.schedule();}});
    document.addEventListener('keyup',event=>{if(event.target.id==='source'&&['ArrowLeft','ArrowRight','Home','End'].includes(event.key))this.schedule();});
    document.addEventListener('mousedown',event=>{if(event.target.id!=='source'&&!this.popup.contains(event.target))this.hide();},true);
    document.addEventListener('focusout',event=>{if(event.target.id==='source')this.hide();});
    document.addEventListener('texstudio-source-sync',()=>{this.hide();this.snippets.session=null;});
    document.addEventListener('scroll',event=>{if(event.target.id==='source')this.hide();},true);
    window.addEventListener('resize',()=>this.hide());
  }
  configure(config) {
    if (this.config.identity!==config.identity) { this.hide(); this.snippets.session=null; }
    this.config=config;
    if(!config.editable||!config.autocomplete)this.hide();
  }
  hide() {
    clearTimeout(this.timer); this.popup.hidden=true; this.result={items:[],query:null};this.selectionExplicit=false;
    const editor=document.getElementById('source');
    editor?.setAttribute('aria-expanded','false'); editor?.removeAttribute('aria-activedescendant');
  }
  schedule(manual=false) {
    clearTimeout(this.timer);
    if(manual)this.refresh(true); else this.timer=setTimeout(()=>this.refresh(false),65);
  }
  refresh(manual=false) {
    const editor=document.getElementById('source');
    if(!editor||document.activeElement!==editor||!this.config.editable||!this.config.autocomplete||this.composing){this.hide();return;}
    // Existing placeholder selections should be replaced by typing, not by an
    // unsolicited suggestion acceptance. Ctrl+Space remains available there.
    if(editor.selectionStart!==editor.selectionEnd&&!manual){this.hide();return;}
    const caret=manual?editor.selectionEnd:editor.selectionStart;
    this.result=suggestions(editor.value,caret,this.catalog,this.config.index,this.config.active,manual);
    if(!this.result.items.length){this.hide();return;}
    this.snapshot={text:editor.value,caret:editor.selectionStart,end:editor.selectionEnd,identity:this.config.identity};
    this.selected=0;this.selectionExplicit=manual;
    this.render();
    editor.setAttribute('aria-autocomplete','list'); editor.setAttribute('aria-controls','completion-list');editor.setAttribute('aria-expanded','true');
    this.popup.hidden=false; this.position(editor); this.highlight();
  }
  render() {
    this.list.replaceChildren();
    for(const [index,item]of this.result.items.entries()) {
      const row=document.createElement('button');row.type='button';row.className='completion-option';row.id=`completion-option-${index}`;row.setAttribute('role','option');row.tabIndex=-1;
      const label=document.createElement('span');label.className='completion-label';label.textContent=item.label;
      const kind=document.createElement('span');kind.className='completion-kind';kind.textContent=item.kind;
      row.append(label,kind);
      row.addEventListener('mousedown',event=>event.preventDefault());
      row.addEventListener('click',()=>{this.selected=index;this.accept();});
      this.list.append(row);
    }
  }
  highlight() {
    for(const [index,row]of [...this.list.children].entries()){
      row.classList.toggle('selected',index===this.selected);row.setAttribute('aria-selected',String(index===this.selected));
    }
    this.detail.textContent=this.result.items[this.selected]?.detail||'';
    document.getElementById('source')?.setAttribute('aria-activedescendant',`completion-option-${this.selected}`);
    this.list.children[this.selected]?.scrollIntoView({block:'nearest'});
  }
  position(editor) {
    const style=getComputedStyle(editor), rect=editor.getBoundingClientRect();
    const mirror=document.createElement('div');
    for(const property of ['fontFamily','fontSize','fontWeight','fontStyle','letterSpacing','lineHeight','paddingTop','paddingLeft','paddingRight','borderLeftWidth','borderTopWidth','tabSize'])mirror.style[property]=style[property];
    Object.assign(mirror.style,{position:'fixed',left:'0',top:'0',visibility:'hidden',whiteSpace:'pre',width:rect.width+'px',boxSizing:'border-box'});
    mirror.textContent=editor.value.slice(0,editor.selectionStart);
    const marker=document.createElement('span');marker.textContent='.';mirror.append(marker);document.body.append(mirror);
    const x=rect.left+marker.offsetLeft-editor.scrollLeft,y=rect.top+marker.offsetTop-editor.scrollTop;
    mirror.remove();
    const width=Math.min(390,rect.width-14),height=Math.min(315,window.innerHeight-70);
    this.popup.style.width=width+'px';this.popup.style.maxHeight=height+'px';
    this.popup.style.left=Math.max(rect.left+2,Math.min(x,rect.right-width-5))+'px';
    const line=parseFloat(style.lineHeight)||23;
    this.popup.style.top=(y+line+height>window.innerHeight-30?Math.max(5,y-height-3):Math.max(rect.top,y+line+3))+'px';
  }
  accept() {
    const editor=document.getElementById('source');
    if(!editor||!this.snapshot||this.snapshot.identity!==this.config.identity||this.snapshot.text!==editor.value||this.snapshot.caret!==editor.selectionStart||this.snapshot.end!==editor.selectionEnd){this.hide();return false;}
    const item=this.result.items[this.selected];if(!item)return false;
    const query=this.result.query;
    const edit=completionEdit(editor.value,query,item,this.config.autoClosing);
    this.applying=true;
    this.snippets.insertTemplate({...edit,selected:''});
    this.applying=false;this.hide();
    if(item.label==='\\begin'||item.label==='\\end')this.schedule(true);
    return true;
  }
  apply(edit) {
    const editor=document.getElementById('source');if(!editor)return;
    if(edit.closeEnvironment) {
      const next=editor.value.slice(0,edit.start)+edit.text+editor.value.slice(edit.end);
      const close=closeEnvironmentEdit(next,edit.caret);
      if(close)edit={...edit,text:edit.text+close.text};
    }
    this.applying=true;window.texstudioApplyingEdit=true;
    editor.focus();editor.setSelectionRange(edit.start,edit.end);
    // Chromium's insertText keeps these transactions in the native undo stack.
    const inserted=document.execCommand?.('insertText',false,edit.text);
    if(!inserted)editor.setRangeText(edit.text,edit.start,edit.end,'end');
    editor.setSelectionRange(edit.caret,edit.selectionEnd??edit.caret);
    if(!inserted)editor.dispatchEvent(new Event('input',{bubbles:true}));
    window.texstudioApplyingEdit=false;this.applying=false;
    this.hide();this.schedule();
  }
  onInput(event) {
    if(event.target.id!=='source'||this.applying||this.composing||event.isComposing||!this.config.editable)return;
    const editor=event.target;
    if(this.config.autoClosing&&editor.selectionStart===editor.selectionEnd){
      const edit=closeEnvironmentEdit(editor.value,editor.selectionStart);
      if(edit){this.apply(edit);return;}
    }
    this.schedule();
  }
  onKeydown(event) {
    if(event.target.id!=='source'||!this.config.editable||event.isComposing||this.composing)return;
    if(this.config.autocomplete&&(event.ctrlKey||event.metaKey)&&(event.code==='Space'||event.key===' ')){
      event.preventDefault();event.stopImmediatePropagation();this.schedule(true);return;
    }
    if(!this.popup.hidden){
      if(event.key==='ArrowDown'||event.key==='ArrowUp'){
        event.preventDefault();event.stopImmediatePropagation();this.selectionExplicit=true;this.selected=(this.selected+(event.key==='ArrowDown'?1:-1)+this.result.items.length)%this.result.items.length;this.highlight();return;
      }
      if((event.key==='Tab'||event.key==='Enter'&&this.selectionExplicit)&&!event.shiftKey){event.preventDefault();event.stopImmediatePropagation();this.accept();return;}
      if(event.key==='Enter')this.hide();
      if(event.key==='Escape'){event.preventDefault();event.stopImmediatePropagation();this.hide();return;}
      if(['ArrowLeft','ArrowRight','Home','End'].includes(event.key))this.hide();
    }
    if(this.config.autoClosing&&!event.ctrlKey&&!event.metaKey&&!event.altKey){
      const editor=event.target;
      const edit=autoClosingEdit(editor.value,editor.selectionStart,editor.selectionEnd,event.key);
      if(edit){event.preventDefault();event.stopImmediatePropagation();this.apply(edit);}
    }
  }
}
