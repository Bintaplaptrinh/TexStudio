// Bundled PDF.js and source editor helpers. No CDN or runtime network service.
document.title = 'TexStudio - LaTex workspace';
const sourceBuffer = import(`${ASSET_BASE}/source-buffer.mjs`).then(module=>new module.SourceBuffer());
let sourceGeneration=0;
window.texstudioSyncSource=async payload=>{
  const generation=++sourceGeneration,buffer=await sourceBuffer;
  for(let i=0;i<100&&!document.getElementById('source');i++)await new Promise(resolve=>setTimeout(resolve,20));
  const editor=document.getElementById('source');
  if(!editor||generation!==sourceGeneration)return;
  if(buffer.sync(editor,payload))document.dispatchEvent(new Event('texstudio-source-sync'));
};
const pdfLibrary = import(`${ASSET_BASE}/pdf.mjs`).then(pdfjs => {
  pdfjs.GlobalWorkerOptions.workerSrc = `${ASSET_BASE}/pdf.worker.mjs`;
  return pdfjs;
});
const snippetController = Promise.all([
  import(`${ASSET_BASE}/snippets.mjs`),
  fetch(`${ASSET_BASE}/snippet-worker.js`).then(response => response.text()),
]).then(([module, workerSource]) => new module.SnippetController(workerSource));
window.texstudioConfigureSnippets = async config => (await snippetController).configure(config);
const assistanceController = Promise.all([
  import(`${ASSET_BASE}/latex-model.mjs`),
  import(`${ASSET_BASE}/editor-assistance.mjs`),
  snippetController,
  Promise.all(['aerijo','macros','environments','packagenames','classnames'].map(async name => {
    const response = await fetch(`${ASSET_BASE}/completion-${name}.json`);
    if (!response.ok) throw new Error(`Could not load ${name} completions`);
    return [name,await response.json()];
  })),
]).then(([model,ui,snippets,data]) => new ui.EditorAssistance(model.createCatalog(Object.fromEntries(data)),snippets));
window.texstudioConfigureEditing = async config => (await assistanceController).configure(config);
let pdfDocument = null;
let pdfScale = 1;
let renderGeneration = 0;
let pendingPdf = null;
let loadingGeneration = 0;

async function paintPdf() {
  const host = document.getElementById('pdf-pages');
  const container = document.getElementById('pdf-container');
  if (!host || !pdfDocument) return;
  const generation = ++renderGeneration;
  const currentDocument = pdfDocument;
  const scroll = container.scrollTop;
  const pages = document.createDocumentFragment();
  for (let n = 1; n <= currentDocument.numPages; n++) {
    const page = await currentDocument.getPage(n);
    if (generation !== renderGeneration) return;
    const viewport = page.getViewport({scale: pdfScale});
    const ratio = window.devicePixelRatio || 1;
    const canvas = document.createElement('canvas');
    canvas.width = Math.floor(viewport.width * ratio);
    canvas.height = Math.floor(viewport.height * ratio);
    canvas.style.width = `${viewport.width}px`;
    canvas.style.height = `${viewport.height}px`;
    canvas.setAttribute('aria-label', `PDF page ${n}`);
    await page.render({canvasContext: canvas.getContext('2d'), viewport,
      transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0]}).promise;
    if (generation !== renderGeneration) return;
    pages.appendChild(canvas);
  }
  host.replaceChildren(pages);
  container.scrollTop = scroll;
  document.getElementById('pdf-placeholder').style.display = 'none';
  document.getElementById('page-count').textContent = `${currentDocument.numPages} ${currentDocument.numPages === 1 ? 'page' : 'pages'}`;
  document.getElementById('zoom-level').textContent = `${Math.round(pdfScale * 100)}%`;
}

window.texstudioRenderWhenReady = async (encoded) => {
  pendingPdf = encoded;
  const generation = ++loadingGeneration;
  try {
    const pdfjs = await pdfLibrary;
    for (let n = 0; !document.getElementById('pdf-pages') && n < 100; n++)
      await new Promise(resolve => setTimeout(resolve, 50));
    if (generation !== loadingGeneration) return;
    const data = Uint8Array.from(atob(pendingPdf), c => c.charCodeAt(0));
    const loaded = await pdfjs.getDocument({data, isEvalSupported: false, useSystemFonts: true}).promise;
    if (generation !== loadingGeneration) { await loaded.loadingTask.destroy(); return; }
    const old = pdfDocument;
    pdfDocument = loaded;
    if (!old) {
      const page = await loaded.getPage(1);
      const width = document.getElementById('pdf-container').clientWidth - 50;
      pdfScale = Math.min(1.5, width / page.getViewport({scale: 1}).width);
    }
    await paintPdf();
    if (old) await old.loadingTask.destroy();
  } catch (error) {
    if (generation !== loadingGeneration) return;
    const placeholder = document.getElementById('pdf-placeholder');
    if (placeholder) {
      placeholder.style.display = 'block';
      placeholder.textContent = `PDF preview: ${error.message}. The last PDF is also available in .texstudio/preview.pdf.`;
    }
    console.error(error);
  }
};
window.texstudioClearPreview = () => {
  loadingGeneration++;
  renderGeneration++;
  const old = pdfDocument;
  pdfDocument = null;
  pendingPdf = null;
  old?.loadingTask.destroy();
  document.getElementById('pdf-pages')?.replaceChildren();
  const placeholder = document.getElementById('pdf-placeholder');
  if (placeholder) placeholder.style.display = 'block';
  const counter = document.getElementById('page-count');
  if (counter) counter.textContent = 'PDF preview';
};
window.texstudioZoom = async delta => { pdfScale = Math.max(.25, Math.min(2.5, pdfScale + delta)); await paintPdf(); };
window.texstudioFit = async () => {
  if (!pdfDocument) return;
  const page = await pdfDocument.getPage(1);
  pdfScale = (document.getElementById('pdf-container').clientWidth - 50) / page.getViewport({scale: 1}).width;
  await paintPdf();
};

document.addEventListener('scroll', event => {
  if (event.target.id !== 'source') return;
  const highlight = document.getElementById('highlight');
  const numbers = document.getElementById('line-numbers');
  if (highlight) { highlight.scrollTop = event.target.scrollTop; highlight.scrollLeft = event.target.scrollLeft; }
  if (numbers) numbers.scrollTop = event.target.scrollTop;
}, true);
document.addEventListener('keydown', event => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') event.preventDefault();
  if (event.target.id === 'source' && event.key === 'Tab') {
    if (window.texstudioSnippetKeydown?.(event)) return;
    event.preventDefault();
    const editor = event.target;
    editor.setRangeText('  ', editor.selectionStart, editor.selectionEnd, 'end');
    editor.dispatchEvent(new Event('input', {bubbles: true}));
  }
  if (event.target.id === 'source' && event.key === 'Escape') window.texstudioSnippetKeydown?.(event);
});
