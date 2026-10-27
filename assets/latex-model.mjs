const MATH_ENV = /^(?:math|displaymath|equation|align|alignat|flalign|gather|multline|eqnarray|split|aligned|alignedat|gathered|array|subarray|cases|matrix|[pbBvV]matrix|smallmatrix|tikzcd)\*?$/;
const VERBATIM_ENV = /^(?:verbatim|Verbatim|lstlisting|minted|comment)\*?$/;

export function escapedAt(text, at) {
  let slashes = 0;
  while (at > 0 && text[--at] === '\\') slashes++;
  return slashes % 2 === 1;
}

export function latexContext(text, caret = text.length) {
  const environments = [], delimiters = [];
  let comment = false, verbatim = false;
  for (let i = 0; i < caret;) {
    const c = text[i];
    if (comment) { if (c === '\n') comment = false; i++; continue; }
    if (verbatim) {
      const end = `\\end{${environments.at(-1)}}`;
      const at = text.indexOf(end, i);
      if (at < 0 || at + end.length > caret) break;
      i = at + end.length; environments.pop(); verbatim = false; continue;
    }
    if (c === '%') { comment = true; i++; continue; }
    if (c === '\\') {
      const rest = text.slice(i, caret);
      const verb = /^\\verb\*?([^A-Za-z\s])/.exec(rest);
      if (verb) {
        const end = text.indexOf(verb[1],i + verb[0].length);
        if (end < 0 || end >= caret) { verbatim = true; break; }
        i = end + 1; continue;
      }
      const env = /^\\(begin|end)\s*\{([^{}]+)\}/.exec(rest);
      if (env) {
        if (env[1] === 'begin') { environments.push(env[2]); verbatim = VERBATIM_ENV.test(env[2]); }
        else { const at = environments.lastIndexOf(env[2]); if (at >= 0) environments.splice(at); }
        i += env[0].length; continue;
      }
      const next = text[i+1];
      if (next === '(' || next === '[') delimiters.push(next);
      if (next === ')' && delimiters.at(-1) === '(' || next === ']' && delimiters.at(-1) === '[') delimiters.pop();
      if (next && !/[A-Za-z@]/.test(next)) { i += 2; continue; }
      i++; while (i < caret && /[A-Za-z@]/.test(text[i])) i++;
      continue;
    }
    if (c === '$') {
      const token = text[i+1] === '$' && i+1 < caret ? '$$' : '$';
      if (delimiters.at(-1) === token) delimiters.pop(); else delimiters.push(token);
      i += token.length; continue;
    }
    i++;
  }
  return {comment,verbatim,environments,delimiters,math:delimiters.length > 0 || environments.some(e=>MATH_ENV.test(e)),tikz:environments.includes('tikzpicture')};
}

function uncomment(text) {
  return text.replace(/%[^\n]*/g,(match,at)=>escapedAt(text,at)?match:' '.repeat(match.length));
}

export function bufferSymbols(text, path = '') {
  const clean = uncomment(text), index = {labels:[],citations:[],commands:[],environments:[],packages:[]};
  for (const m of clean.matchAll(/\\label\s*\{([^{}]+)\}/g)) index.labels.push({name:m[1].trim(),path});
  for (const m of clean.matchAll(/@([A-Za-z]+)\s*[{(]\s*([^,\s{}()]+)\s*,/g)) if (!/^(?:string|comment|preamble)$/i.test(m[1])) index.citations.push({name:m[2],path});
  for (const m of clean.matchAll(/\\(?:newcommand|renewcommand|providecommand|DeclareRobustCommand|DeclareMathOperator)\*?\s*\{?\s*\\([A-Za-z@]+)\s*\}?\s*(?:\[([0-9])\])?/g)) index.commands.push({name:m[1],args:Number(m[2]||0),path});
  for (const m of clean.matchAll(/\\(?:def|gdef|edef|xdef)\s*\\([A-Za-z@]+)((?:#[1-9])*)/g)) index.commands.push({name:m[1],args:(m[2].match(/#/g)||[]).length,path});
  for (const m of clean.matchAll(/\\(?:newenvironment|renewenvironment)\*?\s*\{([^{}]+)\}/g)) index.environments.push({name:m[1],path});
  for (const m of clean.matchAll(/\\(?:usepackage|RequirePackage)(?:\[[^\]]*\])?\s*\{([^{}]+)\}/g)) index.packages.push(...m[1].split(',').map(s=>s.trim()));
  return index;
}

const GREEK = new Set('alpha beta gamma delta epsilon varepsilon zeta eta theta vartheta iota kappa varkappa lambda mu nu xi pi varpi rho varrho sigma varsigma tau upsilon phi varphi chi psi omega Gamma Delta Theta Lambda Xi Pi Sigma Upsilon Phi Psi Omega'.split(' '));
const MATH_COMMAND = new Set('frac dfrac tfrac cfrac sqrt sum prod int oint iint iiint lim limsup liminf infty sin cos tan cot sec csc log ln exp det min max sup inf ker mathbb mathcal mathrm mathbf mathit mathsf mathtt overline underline hat bar vec dot ddot tilde widehat widetilde left right big Big bigg Bigg bigl bigr Bigl Bigr biggl biggr Biggl Biggr binom dbinom tbinom overset underset operatorname displaystyle textstyle scriptscriptstyle scriptstyle substack'.split(' '));

export function createCatalog(data) {
  const commands = new Map();
  for (const [group, scopes] of Object.entries(data.aerijo || {})) {
    // Atom-specific magic comments are not TexStudio configuration commands.
    if (group === 'magicComments') continue;
    for (const [scope,kinds] of Object.entries(scopes)) for (const entries of Object.values(kinds)) for (const entry of entries) {
      if (!entry.displayText.startsWith('\\')) continue;
      const key = entry.displayText;
      if (commands.has(key)) continue;
      commands.set(key,{label:key,body:entry.snippet,detail:entry.description || group,kind:'command',mathOnly:scope.includes('math'),tikzOnly:scope.includes('tikz'),source:'Aerijo/autocomplete-latex'});
    }
  }
  for (const macro of data.macros || []) {
    const label = '\\' + macro.name;
    if (commands.has(label)) continue;
    const raw = macro.arg?.snippet || macro.name;
    // Workshop macro bodies omit the leading slash. Escape only that prefix
    // for the shared template parser; the remaining body keeps its own escapes.
    commands.set(label,{label,body:'\\\\' + raw,detail:macro.detail || 'LaTeX command',kind:'command',mathOnly:GREEK.has(macro.name)||MATH_COMMAND.has(macro.name),source:'LaTeX Workshop'});
  }
  // Keep the basic begin command small; environment-name completion adds its pair.
  commands.set('\\begin',{label:'\\begin',body:'\\\\begin{${1}}$0',detail:'Begin an environment',kind:'command'});
  commands.set('\\end',{label:'\\end',body:'\\\\end{${1}}$0',detail:'Close an environment',kind:'command'});
  return {commands:[...commands.values()],environments:data.environments || [],packages:data.packagenames || [],classes:data.classnames || []};
}

export function completionQuery(text, caret, manual = false) {
  const before = text.slice(0,caret), context = latexContext(text,caret);
  if (context.comment || context.verbatim) return null;
  // An argument may have optional brackets and multiple comma-separated values.
  const arg = /\\(begin|end|usepackage|RequirePackage|documentclass|(?:[A-Za-z]*cite[A-Za-z]*|[A-Za-z]*ref)|input|include|includegraphics|addbibresource|bibliography)(?:\*?)(?:\[[^\]\n]*\])*\{([^{}\n]*)$/.exec(before);
  if (arg) {
    const part = arg[2].split(',').at(-1);
    const query = part.trimStart();
    const start = caret - query.length;
    let kind;
    if (arg[1] === 'begin' || arg[1] === 'end') kind = arg[1] === 'begin' ? 'environment' : 'end';
    else if (arg[1] === 'usepackage' || arg[1] === 'RequirePackage') kind = 'package';
    else if (arg[1] === 'documentclass') kind = 'class';
    else if (arg[1].includes('cite')) kind = 'citation';
    else if (arg[1].endsWith('ref')) kind = 'label';
    else kind = 'file';
    return {kind,query,start,end:caret,context,command:arg[1]};
  }
  const command = /\\[A-Za-z@]*$/.exec(before);
  if (command && !escapedAt(text,command.index)) return {kind:'command',query:command[0],start:command.index,end:caret,context};
  const citation = /(?:^|[^\w@])(@[\w:./-]*)$/.exec(before);
  if (citation && (manual || citation[1].length > 1)) return {kind:'at-citation',query:citation[1].slice(1),start:caret-citation[1].length,end:caret,context};
  return null;
}

function score(label, query) {
  const value=label.toLowerCase(), needle=query.toLowerCase();
  if (value === needle) return 10000;
  if (value.startsWith(needle)) return 2000 - label.length;
  let i=0, cost=0, previous=-1;
  for (const c of needle) { const next=value.indexOf(c,i); if (next<0) return -1; cost += next-previous-1; previous=next;i=next+1; }
  return 500 - cost*3 - label.length;
}

export function suggestions(text, caret, catalog, project = {}, active = '', manual = false) {
  const query = completionQuery(text,caret,manual);
  if (!query) return {query:null,items:[]};
  const own = bufferSymbols(text,active);
  const symbols = key => [...(own[key]||[]),...(project[key]||[]).filter(s=>s.path !== active)];
  let items=[];
  if (query.kind === 'command') {
    items=catalog.commands.filter(s=>(!s.mathOnly||query.context.math)&&(!s.tikzOnly||query.context.tikz));
    items=[...symbols('commands').map(s=>({label:'\\'+s.name,body:'\\\\'+s.name+Array.from({length:s.args||0},(_,i)=>'{${'+(i+1)+'}}').join('')+'$0',kind:'command',detail:`Project · ${s.path || active}`,source:'Project',boost:3000})),...items];
  } else if (query.kind === 'environment' || query.kind === 'end') {
    const names=[...symbols('environments').map(s=>({name:s.name,detail:'Project environment'})),...catalog.environments];
    if (query.kind === 'end') for (const name of [...query.context.environments].reverse()) names.unshift({name,detail:'Open environment'});
    items=names.map(s=>({label:s.name,body:s.name,kind:'environment',detail:s.detail || 'LaTeX environment',boost:s.detail==='Open environment'?4000:0,environment:s.name}));
  } else if (query.kind === 'package' || query.kind === 'class') {
    items=(query.kind==='package'?catalog.packages:catalog.classes).map(s=>({label:s.name,body:s.name,kind:query.kind,detail:s.detail||`LaTeX ${query.kind}`}));
  } else if (query.kind === 'label' || query.kind === 'citation' || query.kind === 'at-citation') {
    items=symbols(query.kind==='label'?'labels':'citations').map(s=>({label:s.name,body:query.kind==='at-citation'?'\\\\cite{'+s.name+'}$0':s.name,kind:query.kind==='label'?'label':'citation',detail:s.path || active}));
  } else if (query.kind === 'file') {
    let files=project.files||[];
    if (/^(input|include)$/.test(query.command)) files=files.filter(f=>f.endsWith('.tex'));
    if (/^(bibliography|addbibresource)$/.test(query.command)) files=files.filter(f=>f.endsWith('.bib'));
    if (query.command==='includegraphics') files=files.filter(f=>/\.(png|jpe?g|pdf|eps|svg)$/i.test(f));
    items=files.map(f=>({label:f,body:/^(include|bibliography)$/.test(query.command)?f.replace(/\.(tex|bib)$/,''):f,kind:'file',detail:'Project file'}));
  }
  const unique=new Map();
  for (const item of items) {
    const rank=score(item.label,query.query);
    if (rank<0 || unique.has(item.label)) continue;
    unique.set(item.label,{...item,rank:rank+(item.boost||0)});
  }
  return {query,items:[...unique.values()].sort((a,b)=>b.rank-a.rank||a.label.localeCompare(b.label)).slice(0,60)};
}

function matchingEnd(text, caret, name) {
  const stack=[name], tail=uncomment(text.slice(caret));
  for (const m of tail.matchAll(/\\(begin|end)\s*\{([^{}]+)\}/g)) {
    if (m[1]==='begin') stack.push(m[2]);
    else {
      if (stack.at(-1)!==m[2]) return false;
      stack.pop(); if (!stack.length) return true;
    }
  }
  return false;
}

export function closeEnvironmentEdit(text, caret) {
  if(text[caret-1]!=='}')return null;
  const match=/\\begin\{([A-Za-z@][\w*@-]*)\}$/.exec(text.slice(0,caret));
  if (!match || escapedAt(text,match.index)) return null;
  const context=latexContext(text,match.index);
  if (context.comment||context.verbatim||matchingEnd(text,caret,match[1])) return null;
  return {start:caret,end:caret,text:`\\end{${match[1]}}`,caret,selectionEnd:caret};
}

export function autoClosingEdit(text,start,end,key) {
  const context=latexContext(text,start), before=text.slice(0,start), after=text.slice(end), selected=text.slice(start,end);
  if (context.comment||context.verbatim) return null;
  if (key==='Backspace' && start===end) {
    for (const [open,close] of [['\\left\\{','\\right\\}'],['\\left(','\\right)'],['\\left[','\\right]'],['$$','$$'],['\\(','\\)'],['\\[','\\]'],['\\{','\\}'],['{','}'],['[',']'],['(',')'],['$','$']]) {
      if (before.endsWith(open)&&after.startsWith(close)) return {start:start-open.length,end:end+close.length,text:'',caret:start-open.length};
    }
    return null;
  }
  if (key==='Enter' && start===end) {
    const match=/(^|\n)([ \t]*)\\begin\{([^{}]+)\}(?:\[[^\]\n]*\]|\{[^{}\n]*\})*$/.exec(before);
    if (!match) return null;
    const closing=new RegExp('^[ \\t\\r\\n]*'+escapeRegex(`\\end{${match[3]}}`)).exec(after);
    const whitespace=closing ? /^[ \t\r\n]*/.exec(after)[0].length : 0;
    return {start,end:end+whitespace,text:`\n${match[2]}  ${selected}\n${match[2]}`,caret:start+1+match[2].length+2};
  }
  if (start===end && key==='$' && before.endsWith('$') && !before.endsWith('$$') && after.startsWith('$') && !escapedAt(text,start-1)) {
    return {start:start-1,end:end+1,text:'$$$$',caret:start+1};
  }
  if(start===end&&key==='\\'&&/^\\[)\]}]/.test(after))return {start,end:end+1,text:'\\',caret:start+1};
  if(start===end&&[')',']','}'].includes(key)) {
    const closing='\\right'+(key==='}'?'\\':'')+key;
    if(after.startsWith(closing))return {start,end:end+closing.length,text:closing,caret:start+closing.length};
  }
  if (start===end && ['}',']',')','$'].includes(key) && after.startsWith(key) && (key!=='$'||!escapedAt(text,start))) return {start,end:end+1,text:key,caret:start+1,closeEnvironment:key==='}'};
  if(start===end&&key==='}') {
    const close=closeEnvironmentEdit(before+'}'+after,start+1);
    if(close)return {start,end,text:'}'+close.text,caret:start+1};
  }
  if (key==='{' || key==='[' || key==='(' || key==='$' || key==='|' && before.endsWith('\\left')) {
    let close={ '{':'}','[':']','(':')','$':'$','|':'|' }[key];
    const escaped=escapedAt(text,start);
    if (key==='$' && escaped) return null;
    if (before.endsWith('\\left') || before.endsWith('\\left\\')) close='\\right'+(escaped&&key==='{'?'\\':'')+close;
    else if (escaped && ['{','[','('].includes(key)) close='\\'+close;
    // Do not pair a delimiter into the middle of an identifier.
    if (!selected && /^[\p{L}\p{N}]/u.test(after)) return null;
    return {start,end,text:key+selected+close,caret:start+1,selectionEnd:start+1+selected.length};
  }
  return null;
}

export function completionEdit(text, query, item, autoClose=true) {
  let start=query.start,end=query.end,body=item.body;
  const after=text.slice(end);
  if (query.kind==='environment') {
    if (after.startsWith('}')) end++;
    const base=text.slice(0,start)+item.label+'}'+text.slice(end);
    const caret=start+item.label.length+1;
    const close=autoClose?closeEnvironmentEdit(base,caret):null;
    body=item.label+'}'+(close?'$0'+close.text:'$0');
  } else if (['end','package','class','label','citation','file'].includes(query.kind)) {
    if (after.startsWith('}')) { end++; body+='}'; }
    body+='$0';
  }
  return {start,end,body};
}

function escapeRegex(value) { return value.replace(/[.*+?^${}()|[\]\\]/g,'\\$&'); }
