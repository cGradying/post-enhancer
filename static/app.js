// Post Prep: shared core (photos, settings, export) + the Post Prep tool.
// template.js registers the Template tool on App.tools.
const $ = id => document.getElementById(id);
const esc = s => String(s).replace(/[&<>"']/g, c => `&#${c.charCodeAt(0)};`);
const icon = id => `<svg class="ico" aria-hidden="true"><use href="#i-${id}"/></svg>`;
const store = {
  get(k) { try { return JSON.parse(localStorage.getItem(k)); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); return true; } catch { return false; } },
};

const PRESETS = [
  {id:'ig_portrait', group:'Instagram', name:'Feed portrait 4:5', w:1080, h:1350, on:true},
  {id:'ig_tall', group:'Instagram', name:'Feed tall 3:4', w:1080, h:1440},
  {id:'ig_square', group:'Instagram', name:'Feed square', w:1080, h:1080},
  {id:'ig_landscape', group:'Instagram', name:'Feed landscape', w:1080, h:566},
  {id:'ig_story', group:'Instagram', name:'Story / Reel', w:1080, h:1920},
  {id:'fb_feed', group:'Facebook', name:'Feed', long:2048, on:true},
  {id:'fb_story', group:'Facebook', name:'Story', w:1080, h:1920},
];
const UNITS = {padAmt:'%', textSpace:'%', logoSize:'%', wmSize:'%', logoAlpha:'%', wmAlpha:'%', border:' px', radius:' px', textSize:' px', quality:''};
const BROWSER_TYPES = /^image\/(jpeg|png|webp|gif|bmp|avif|svg\+xml)$/;
const SERVER_EXT = /\.(tiff?|heic|heif|avif|dng|arw|cr2|cr3|nef|raf|orf|rw2)$/i;

const App = {
  photos: [], current: -1, tool: 'post', view: 'single', caps: {ai_upscale: false, heic: false}, tools: {}, busy: false,
  get photo() { return this.photos[this.current]; },
};

// ---------- segmented controls + generic settings ----------
const segVal = id => { const b = $(id).querySelector('.on'); return b && (b.dataset.v ?? b.dataset.r ?? b.dataset.c); };
function setSeg(id, value) { $(id).querySelectorAll('button').forEach(b => b.classList.toggle('on', (b.dataset.v ?? b.dataset.r ?? b.dataset.c) === value)); }
document.querySelectorAll('aside .seg, #viewSeg').forEach(seg => seg.addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b || !seg.id) return;
  seg.querySelectorAll('button').forEach(x => x.classList.toggle('on', x === b));
  seg.dispatchEvent(new Event('seg', {bubbles: true}));
}));

const SETTINGS_KEY = 'postprep.v2';
function saveSettings() {
  const s = {inputs: {}, segs: {}};
  document.querySelectorAll('aside input[id], aside select[id]').forEach(i => {
    if (i.type === 'file' || i.id === 'tplPick') return;
    s.inputs[i.id] = i.type === 'checkbox' ? i.checked : i.value;
  });
  document.querySelectorAll('aside .seg[id], #viewSeg').forEach(seg => s.segs[seg.id] = segVal(seg.id));
  s.presets = chosen(); s.tool = App.tool;
  store.set(SETTINGS_KEY, s);
}
function loadSettings() {
  const s = store.get(SETTINGS_KEY); if (!s) return;
  for (const [id, v] of Object.entries(s.inputs || {})) { const i = $(id); if (!i) continue; if (i.type === 'checkbox') i.checked = v; else i.value = v; }
  for (const [id, v] of Object.entries(s.segs || {})) if ($(id) && v != null) setSeg(id, v);
  if (s.presets) presetBox.querySelectorAll('input').forEach(i => i.checked = s.presets.includes(i.value));
  if (s.tool) App.tool = s.tool;
}

let frame = 0;
function changed() {
  syncLabels();
  saveSettings();
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(render);
}
document.querySelector('aside').addEventListener('input', e => { if (e.target.type !== 'file') changed(); });
document.querySelector('aside').addEventListener('seg', changed);
$('viewSeg').addEventListener('seg', () => { App.view = segVal('viewSeg'); changed(); });

function syncLabels() {
  document.querySelectorAll('[data-out]').forEach(o => { const i = $(o.dataset.out); if (i) o.textContent = i.value + (UNITS[i.id] ?? ''); });
  const r = segVal('ratios');
  $('customRatio').hidden = r !== 'custom';
  $('splitFields').hidden = segVal('splitSeg') !== 'on';
  document.querySelectorAll('.swatch').forEach(s => s.classList.toggle('on', s.dataset.c === $('pad').value));
  $('splitTag').textContent = splitOn() ? `${$('cols').value} × ${ratio().join(':')}` : 'Off';
  $('outCount').textContent = `${chosen().length} selected`;
  $('sample').innerHTML = samplName();
  App.tools[App.tool]?.sync?.();
}

// ---------- Post Prep settings ----------
const presetBox = $('presets');
for (const g of ['Instagram', 'Facebook']) {
  presetBox.insertAdjacentHTML('beforeend', `<div class="grp">${g.toUpperCase()}</div>`);
  for (const p of PRESETS.filter(p => p.group === g)) {
    const size = p.long ? `${p.long} long side` : `${p.w}×${p.h}`;
    presetBox.insertAdjacentHTML('beforeend', `<label class="opt" data-id="${p.id}"><input type="checkbox" value="${p.id}" ${p.on ? 'checked' : ''}><span>${p.name}</span><small class="num">${size}</small></label>`);
  }
}
const chosen = () => [...presetBox.querySelectorAll('input:checked')].map(i => i.value);
const splitOn = () => segVal('splitSeg') === 'on';
const clampCols = n => Math.min(20, Math.max(1, n || 1));
const ratio = () => segVal('ratios') === 'custom' ? [Math.max(1, +$('rw').value || 1), Math.max(1, +$('rh').value || 1)] : segVal('ratios').split(':').map(Number);
presetBox.addEventListener('mouseover', e => highlight(e.target.closest('.opt')?.dataset.id));
presetBox.addEventListener('focusin', e => highlight(e.target.closest('.opt')?.dataset.id));
presetBox.addEventListener('mouseleave', () => highlight(null));
document.querySelectorAll('[data-pick]').forEach(b => b.addEventListener('click', () => {
  presetBox.querySelectorAll('input').forEach(i => i.checked = b.dataset.pick === 'all' || (b.dataset.pick === 'ig' && i.value.startsWith('ig_')));
  changed();
}));
document.querySelectorAll('[data-step]').forEach(b => b.addEventListener('click', () => { $('cols').value = clampCols(+$('cols').value + +b.dataset.step); changed(); }));
$('autoCols').addEventListener('click', () => {
  const p = App.photo; if (!p?.w) return;
  const [rw, rh] = ratio();
  $('cols').value = clampCols(Math.round(p.w * rh / (p.h * rw)));
  changed();
});
document.querySelectorAll('.swatch').forEach(s => s.addEventListener('click', () => { $('pad').value = s.dataset.c; changed(); }));
$('tokens').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  const i = $('rename'), at = i.selectionStart ?? i.value.length;
  i.value = i.value.slice(0, at) + b.textContent + i.value.slice(i.selectionEnd ?? at);
  i.focus(); changed();
});

// ---------- tabs ----------
document.querySelector('.tabs').addEventListener('click', e => {
  const b = e.target.closest('[data-tool]'); if (!b || b.dataset.tool === App.tool) return;
  const go = () => { App.tool = b.dataset.tool; applyTool(); saveSettings(); render(); };
  document.startViewTransition && !matchMedia('(prefers-reduced-motion: reduce)').matches ? document.startViewTransition(go) : go();
});
function applyTool() {
  document.querySelectorAll('.tabs button').forEach(b => { const on = b.dataset.tool === App.tool; b.classList.toggle('on', on); b.setAttribute('aria-selected', on); });
  $('tool-post').hidden = App.tool !== 'post';
  $('tool-template').hidden = App.tool !== 'template';
  syncLabels();
}

// ---------- photos ----------
$('choose').addEventListener('click', () => $('pick').click());
$('pick').addEventListener('change', e => { add(e.target.files); e.target.value = ''; });
let dragDepth = 0;
addEventListener('dragenter', e => { e.preventDefault(); if (++dragDepth === 1) $('stage').classList.add('over'); });
addEventListener('dragleave', () => { if (--dragDepth <= 0) { dragDepth = 0; $('stage').classList.remove('over'); } });
addEventListener('dragover', e => e.preventDefault());
addEventListener('drop', e => { e.preventDefault(); dragDepth = 0; $('stage').classList.remove('over'); add(e.dataTransfer.files); });
addEventListener('paste', e => {
  if (e.target.closest?.('input,textarea')) return;
  const files = [...e.clipboardData.items].filter(i => i.kind === 'file').map(i => i.getAsFile());
  if (files.length) add(files);
});

const accepted = f => BROWSER_TYPES.test(f.type) || f.type === 'image/tiff' || SERVER_EXT.test(f.name) || /^image\/hei[cf]$/.test(f.type);
function add(list) {
  const all = [...list], files = all.filter(accepted), skipped = all.length - files.length;
  if (!files.length) return status(skipped ? `Skipped ${skipped} file${skipped > 1 ? 's' : ''} that aren't images this app can read.` : '', 'err');
  for (const file of files) {
    const p = {file, url: URL.createObjectURL(file), img: new Image(), scale: 1};
    p.img.onload = () => { p.w ??= p.img.naturalWidth; p.h ??= p.img.naturalHeight; p.scale = p.img.naturalWidth / p.w; p.ready = true; renderSoon(); };
    p.img.onerror = () => serverPreview(p);
    if (BROWSER_TYPES.test(file.type) && !SERVER_EXT.test(file.name)) p.img.src = p.url; else serverPreview(p);
    App.photos.push(p);
  }
  if (App.current < 0) App.current = 0;
  status(skipped ? `Added ${files.length}. Skipped ${skipped} that aren't images this app can read.` : '');
  render();
}

// Formats browsers can't draw (HEIC, TIFF, RAW) get a JPEG preview from the
// server, at most 3 at a time so a big HEIC drop doesn't flood it.
const previewQueue = []; let previewsRunning = 0;
function serverPreview(p) {
  if (p.previewing) return remove(App.photos.indexOf(p), `Couldn't read ${p.file.name}.`);
  p.previewing = true; previewQueue.push(p); pumpPreviews();
}
async function pumpPreviews() {
  while (previewsRunning < 3 && previewQueue.length) {
    const p = previewQueue.shift(); previewsRunning++;
    (async () => {
      try {
        const fd = new FormData(); fd.append('file', p.file, p.file.name);
        const res = await fetch('/api/preview', {method: 'POST', body: fd});
        if (!res.ok) throw new Error(await res.text());
        const [w, h] = (res.headers.get('x-source-size') || '').split('x').map(Number);
        if (w && h) { p.w = w; p.h = h; }
        p.preview = URL.createObjectURL(await res.blob());
        p.img.src = p.preview;
      } catch (err) {
        remove(App.photos.indexOf(p), `Couldn't read ${p.file.name}: ${String(err.message || err).slice(0, 140)}`);
      } finally { previewsRunning--; pumpPreviews(); }
    })();
  }
}

function remove(i, msg) {
  if (i < 0) return;
  const p = App.photos[i];
  URL.revokeObjectURL(p.url); if (p.preview) URL.revokeObjectURL(p.preview);
  App.photos.splice(i, 1);
  App.current = Math.min(App.current, App.photos.length - 1);
  if (msg) status(msg, 'err');
  render();
}
const go = d => { if (App.photos.length) { App.current = (App.current + d + App.photos.length) % App.photos.length; render(); } };
$('prev').addEventListener('click', () => go(-1));
$('next').addEventListener('click', () => go(1));
$('clearAll').addEventListener('click', () => { while (App.photos.length) remove(0); status(''); });
addEventListener('keydown', e => {
  if (e.target.closest('input,textarea,select')) return;
  if (e.key === 'ArrowLeft') go(-1);
  else if (e.key === 'ArrowRight') go(1);
  else if ((e.key === 'Delete' || e.key === 'Backspace') && App.current >= 0) { e.preventDefault(); remove(App.current); }
});
$('strip').addEventListener('click', e => {
  const rm = e.target.closest('[data-rm]');
  if (rm) { e.stopPropagation(); return remove(+rm.dataset.rm); }
  if (e.target.closest('.add')) return $('pick').click();
  const b = e.target.closest('[data-i]'); if (b) { App.current = +b.dataset.i; render(); }
});

// Draw a source-pixel rect of a photo, whatever resolution its preview has.
function drawPhoto(ctx, p, sx, sy, sw, sh, dx, dy, dw, dh) {
  const s = p.scale || 1;
  ctx.drawImage(p.img, sx * s, sy * s, sw * s, sh * s, dx, dy, dw, dh);
}
function canvasFor(cw, ch, max = 900) {
  const c = document.createElement('canvas'), s = Math.min(1, max / Math.max(cw, ch));
  c.width = Math.max(1, Math.round(cw * s)); c.height = Math.max(1, Math.round(ch * s));
  return [c, c.getContext('2d'), s];
}

// ---------- geometry: mirrors src/split.rs and media::fit ----------
function splitLayout(w, h, cols, [rw, rh]) {
  const [cw, ch] = w * rh >= h * cols * rw ? [w, Math.ceil(w * rh / (cols * rw))] : [Math.ceil(h * cols * rw / rh), h];
  return {cw, ch, ox: Math.floor((cw - w) / 2), oy: Math.floor((ch - h) / 2), cut: i => Math.floor(i * cw / cols)};
}
function fitRect(w, h, p) {
  const [tw, th] = p.long ? (w >= h ? [p.long, Math.floor(p.long * h / w)] : [Math.floor(p.long * w / h), p.long]) : [p.w, p.h];
  const [cw, ch] = w * th > h * tw ? [Math.floor(h * tw / th), h] : [w, Math.floor(w * th / tw)];
  const up = segVal('upSeg') !== 'off';
  return {x: Math.floor((w - cw) / 2), y: Math.floor((h - ch) / 2), cw, ch, small: cw < tw, out: cw > tw || up ? [tw, th] : [cw, ch]};
}

// ---------- render ----------
function renderSoon() { cancelAnimationFrame(frame); frame = requestAnimationFrame(render); }
function render() {
  const n = App.photos.length, p = App.photo;
  $('empty').hidden = !!n; $('strip').hidden = !n; $('clearAll').hidden = !n;
  $('prev').disabled = $('next').disabled = n < 2;
  $('photoName').textContent = p ? p.file.name : 'No photos';
  $('photoDims').textContent = p?.w ? `${p.w}×${p.h} · ${App.current + 1} of ${n}` : p ? 'Reading…' : '';
  $('viewSeg').hidden = App.tool !== 'template' || !n;
  $('outputs').hidden = !n || App.tool !== 'post';
  $('tplView').hidden = !n || App.tool !== 'template';
  renderStrip(); updateExport(); $('sample').innerHTML = samplName();
  if (!n) return paintFades();
  if (App.tool === 'template') return App.tools.template?.render();
  renderPost();
}

function renderPost() {
  const p = App.photo, out = $('outputs');
  out.innerHTML = '';
  if (!p.ready) { out.innerHTML = '<p class="skip">Reading photo…</p>'; return; }
  let i = 0;
  if (splitOn()) {
    const wide = p.w * 10 >= p.h * 15;
    const block = document.createElement('section'); block.className = 'block split-view'; block.style.setProperty('--i', i++);
    if ($('wideOnly').checked && !wide) {
      block.innerHTML = `<h3>Carousel split</h3><p class="skip">This photo isn't 1.5:1 or wider, so it won't be split. Untick "Only split wide photos" to split it anyway.</p>`;
    } else {
      const cols = clampCols(+$('cols').value), r = ratio(), L = splitLayout(p.w, p.h, cols, r);
      const panelW = Math.floor(L.cw / cols), up = segVal('upSeg') !== 'off';
      const outW = up ? 1080 : Math.min(1080, panelW), outH = Math.round(outW * r[1] / r[0]);
      block.innerHTML = `<h3>Carousel <span class="num">${cols} × ${outW}×${outH}${L.cw > p.w || L.ch > p.h ? ' · padded' : ''}</span></h3>`;
      const [c, ctx, s] = canvasFor(L.cw, L.ch, 1400);
      ctx.fillStyle = $('pad').value; ctx.fillRect(0, 0, c.width, c.height);
      drawPhoto(ctx, p, 0, 0, p.w, p.h, L.ox * s, L.oy * s, p.w * s, p.h * s);
      ctx.strokeStyle = '#a66bff'; ctx.lineWidth = Math.max(2, c.width / 400); ctx.setLineDash([ctx.lineWidth * 3, ctx.lineWidth * 2]);
      for (let k = 1; k < cols; k++) { const x = L.cut(k) * s; ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, c.height); ctx.stroke(); }
      block.append(c);
      const row = document.createElement('div'); row.className = 'carousel';
      for (let k = 0; k < cols; k++) {
        const x0 = L.cut(k), x1 = L.cut(k + 1), [pc, pctx] = canvasFor(x1 - x0, L.ch, 400);
        pctx.drawImage(c, x0 * s, 0, (x1 - x0) * s, c.height, 0, 0, pc.width, pc.height);
        const fig = document.createElement('figure'); fig.style.setProperty('--i', k); fig.append(pc);
        fig.insertAdjacentHTML('beforeend', `<figcaption>${k + 1}/${cols}</figcaption>`); row.append(fig);
      }
      block.append(row);
      if (panelW < 1080) block.insertAdjacentHTML('beforeend', `<p class="skip" style="color:var(--warn);margin-top:8px">Panels are ${panelW} px wide, under 1080. ${up ? `They'll be upscaled with ${segVal('upSeg') === 'ai' ? 'AI' : 'Lanczos'}.` : 'They export at that size; turn on upscaling to reach 1080.'}</p>`);
    }
    out.append(block);
  }

  const picks = PRESETS.filter(x => chosen().includes(x.id));
  if (picks.length) {
    const block = document.createElement('section'); block.className = 'block'; block.style.setProperty('--i', i++);
    block.innerHTML = `<h3>Files</h3>`;
    const tiles = document.createElement('div'); tiles.className = 'tiles';
    picks.forEach((pr, k) => {
      const r = fitRect(p.w, p.h, pr), [c, ctx] = canvasFor(r.cw, r.ch, 600);
      drawPhoto(ctx, p, r.x, r.y, r.cw, r.ch, 0, 0, c.width, c.height);
      const fig = document.createElement('figure'); fig.className = 'tile'; fig.dataset.id = pr.id; fig.style.setProperty('--i', k); fig.append(c);
      const up = segVal('upSeg');
      fig.insertAdjacentHTML('beforeend', `<figcaption><b>${pr.group} ${pr.name.toLowerCase()}</b><span class="num">${r.out.join('×')}</span></figcaption>`
        + (r.small ? (up !== 'off' ? `<span class="badge">UPSCALED ×${(r.out[0] / r.cw).toFixed(1)}</span>` : `<span class="warn">Too small, kept at native size</span>`) : ''));
      tiles.append(fig);
    });
    block.append(tiles); out.append(block);
  }
  const orig = +segVal('origSeg');
  if (orig) {
    const block = document.createElement('section'); block.className = 'block'; block.style.setProperty('--i', i++);
    block.innerHTML = `<h3>Upscaled original <span class="num">${p.w * orig}×${p.h * orig}</span></h3>`;
    out.append(block);
  }
  if (!picks.length && !splitOn() && !orig) out.innerHTML = '<p class="skip">Pick at least one output on the left.</p>';
  highlight(App.hl);
}
function highlight(id) {
  App.hl = id;
  presetBox.querySelectorAll('.opt').forEach(o => o.classList.toggle('hl', o.dataset.id === id));
  document.querySelectorAll('.tile[data-id]').forEach(t => t.classList.toggle('hl', t.dataset.id === id));
  if (id) document.querySelector(`.tile[data-id="${id}"]`)?.scrollIntoView({block: 'nearest', behavior: 'smooth'});
}

let stripKey = '';
function renderStrip() {
  const key = App.photos.map(p => p.url + (p.ready ? 1 : 0)).join('|') + '#' + App.current;
  if (key === stripKey) return;
  stripKey = key;
  $('strip').innerHTML = App.photos.map((p, i) => `<button type="button" data-i="${i}" aria-current="${i === App.current}" aria-label="${esc(p.file.name)}">${p.ready ? `<img src="${p.preview || p.url}" alt="">` : '<span class="loading">…</span>'}<span class="x" data-rm="${i}" aria-label="Remove ${esc(p.file.name)}">${icon('x')}</span></button>`).join('')
    + `<button type="button" class="add" aria-label="Add more photos">${icon('plus')}</button>`;
  $('strip').querySelector('[aria-current=true]')?.scrollIntoView({block: 'nearest', inline: 'nearest'});
}

// ---------- names ----------
const today = () => { const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; };
const stemOf = n => n.replace(/^.*[\\/]/, '').replace(/\.[^.]*$/, '').replace(/[^\p{L}\p{N}._-]/gu, '_').replace(/^[_.]+|[_.]+$/g, '') || 'photo';
function renameJs(pattern, name, n, preset) {
  let out = (pattern.trim() || '{name}_{preset}').replaceAll('{name}', name).replaceAll('{event}', $('event').value).replaceAll('{date}', today()).replaceAll('{preset}', preset);
  out = out.replace(/\{n(?::(\d+))?\}/g, (_, w) => String(n).padStart(+w || 0, '0'));
  return out.replace(/[^\p{L}\p{N}._-]/gu, '_').replace(/^[_.]+|[_.]+$/g, '') || 'photo';
}
function samplName() {
  const p = App.photos[0], preset = App.tool === 'template' ? 'framed' : (chosen()[0] || '1of2');
  const ext = segVal('fmtSeg') === 'png' ? 'png' : 'jpg';
  return `First file: <b>${esc(renameJs($('rename').value, stemOf(p ? p.file.name : 'IMG_0001.jpg'), 1, preset))}.${ext}</b>`;
}

// ---------- export ----------
function updateExport() {
  const n = App.photos.length, tool = App.tools[App.tool];
  const outs = App.tool === 'post' ? chosen().length + (splitOn() ? 1 : 0) + (+segVal('origSeg') ? 1 : 0) : 1;
  const reading = App.photos.some(p => !p.ready);
  $('export').disabled = !n || !outs || App.busy || reading;
  $('exportLabel').textContent = App.busy ? 'Exporting…' : !n ? 'Add photos to export' : reading ? 'Reading photos…' : !outs ? 'Pick an output first' : `Export ${n} photo${n > 1 ? 's' : ''}${tool?.label ? ' ' + tool.label : ''}`;
}

function baseOptions() {
  return {
    tool: App.tool, format: segVal('fmtSeg'), quality: +$('quality').value, upscale: segVal('upSeg'),
    enhance: $('enhance').checked, rename: $('rename').value, event: $('event').value, date: today(),
    meta: {artist: $('artist').value, copyright: $('copyright').value},
  };
}

// A big batch goes up in parts, so no single request is huge and a dropped connection only costs one part.
const PART_BYTES = 250e6, PART_FILES = 80;
function parts(photos) {
  const out = []; let cur = [], bytes = 0;
  for (const p of photos) {
    if (cur.length && (bytes + p.file.size > PART_BYTES || cur.length >= PART_FILES)) { out.push(cur); cur = []; bytes = 0; }
    cur.push(p); bytes += p.file.size;
  }
  if (cur.length) out.push(cur);
  return out;
}
const sleep = ms => new Promise(r => setTimeout(r, ms));
const exportStamp = () => { const d = new Date(), p = n => String(n).padStart(2, '0'); return `Post Prep ${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}${p(d.getMinutes())}`; };

$('export').addEventListener('click', async () => {
  App.busy = true; updateExport(); $('reveal').hidden = true;
  const n = App.photos.length, base = baseOptions(), toFolder = App.caps.local && !$('asZip').checked;
  try {
    const layers = {};
    if (App.tool === 'post') {
      base.presets = chosen();
      base.split = splitOn() ? {cols: clampCols(+$('cols').value), rw: ratio()[0], rh: ratio()[1], pad: $('pad').value, wide_only: $('wideOnly').checked} : null;
      base.originals = +segVal('origSeg') || null;
    } else {
      const t = await App.tools.template.payload();
      base.layout = t.layout; layers.under = t.under; layers.over = t.over;
    }
    if (toFolder) { base.out_dir = $('outDir').value.trim() || App.caps.default_dir; base.out_name = exportStamp(); }

    const groups = parts(App.photos);
    let done = 0, folder = '', saved = 0;
    for (const [gi, group] of groups.entries()) {
      const fd = new FormData();
      fd.append('options', JSON.stringify({...base, offset: done}));
      if (layers.under) fd.append('under', layers.under, 'under.png');
      if (layers.over) fd.append('over', layers.over, 'over.png');
      group.forEach(p => fd.append('files', p.file, p.file.name));
      const tag = groups.length > 1 ? ` ${gi + 1} of ${groups.length}` : '';

      let result;
      for (let attempt = 1; ; attempt++) {
        try {
          progress(`Uploading${tag}`, done / n);
          const {id} = await upload(fd, f => progress(`Uploading${tag}`, (done + f * group.length) / n, `${Math.round((done + f * group.length) / n * 100)}%`));
          progress(`Processing${tag}`, 0, `${done} / ${n}`);
          result = await poll(id, (d, t) => progress(`Processing${tag}`, (done + d) / n, `${done + d} / ${n}`));
          break;
        } catch (err) {
          if (!err.retry || attempt >= 3) throw err;
          status(`Part ${gi + 1} dropped, trying again (${attempt}/2)…`, 'err'); await sleep(1500 * attempt);
        }
      }
      if (result.state === 'saved') { folder = result.folder; saved += result.files; }
      else {
        const a = document.createElement('a'); a.href = `/api/batches/${result.batch_id}`; a.download = `post-prep-${result.batch_id}${groups.length > 1 ? `-part${gi + 1}` : ''}.zip`;
        document.body.append(a); a.click(); a.remove(); if (gi < groups.length - 1) await sleep(500);
      }
      done += group.length;
    }
    progress('Done', 1, '');
    if (folder) {
      status(`Done. ${n} photo${n > 1 ? 's' : ''} saved as ${saved} files in ${folder}.`, 'ok');
      $('reveal').hidden = false; $('reveal').dataset.path = folder;
    } else status(`Done. ${n} photo${n > 1 ? 's' : ''} exported to your Downloads${groups.length > 1 ? ` as ${groups.length} zips` : ''}.`, 'ok');
    loadHistory();
  } catch (err) {
    if (err.message === 'login') { location.href = '/login'; return; }
    status(`Export failed: ${err.message}. Your photos are still here, so try again.`, 'err');
  } finally {
    App.busy = false; setTimeout(() => $('prog').hidden = true, 900); updateExport();
  }
});

const local = (url, body) => fetch(url, {method: 'POST', headers: {'X-Post-Prep': '1', ...(body ? {'Content-Type': 'application/json'} : {})}, body: body && JSON.stringify(body)});
$('reveal').addEventListener('click', async () => { const r = await local('/api/reveal', {path: $('reveal').dataset.path}); if (!r.ok) status(await r.text(), 'err'); });
$('outPick').addEventListener('click', async () => {
  $('outPick').disabled = true;
  try {
    const r = await local('/api/choose-folder');
    if (!r.ok) throw new Error(await r.text());
    const j = await r.json();
    if (j.path) { $('outDir').value = j.path; changed(); }
  } catch (err) { status(`Couldn't open the folder picker: ${err.message}`, 'err'); }
  $('outPick').disabled = false;
});

function upload(fd, onProgress) {
  return new Promise((resolve, reject) => {
    const x = new XMLHttpRequest();
    x.open('POST', '/api/jobs'); x.setRequestHeader('X-Post-Prep', '1');
    x.upload.onprogress = e => e.lengthComputable && onProgress(e.loaded / e.total);
    x.onload = () => {
      if (x.status === 401) return reject(new Error('login'));
      if (x.status === 413) return reject(new Error('one photo is bigger than the upload limit'));
      if (x.status !== 200) return reject(Object.assign(new Error(x.responseText || `HTTP ${x.status}`), {retry: x.status >= 500}));
      resolve(JSON.parse(x.responseText));
    };
    x.onerror = () => reject(Object.assign(new Error('the connection dropped'), {retry: true}));
    x.send(fd);
  });
}
async function poll(id, onProgress) {
  for (;;) {
    const res = await fetch(`/api/jobs/${id}`);
    if (res.status === 401) throw new Error('login');
    const j = await res.json();
    onProgress(j.done, j.total);
    if (j.state === 'done' || j.state === 'saved') return j;
    if (j.state === 'failed') throw new Error(j.error);
    await new Promise(r => setTimeout(r, 300));
  }
}
function progress(stage, frac, count = '') {
  $('prog').hidden = false; $('progStage').textContent = stage; $('progCount').textContent = count;
  $('progBar').classList.toggle('indet', stage === 'Processing' && frac === 0);
  $('progBar').firstElementChild.style.transform = `scaleX(${frac})`;
}
function status(text, kind) { $('status').textContent = text; $('status').className = 'status' + (kind ? ' ' + kind : ''); }

async function loadHistory() {
  try {
    const res = await fetch('/api/batches'); if (!res.ok) return;
    const ids = await res.json();
    $('histCount').textContent = ids.length || '';
    $('history').innerHTML = ids.length ? ids.slice(0, 50).map(id => {
      const d = new Date(+id);
      return `<li><a href="/api/batches/${id}" download>${d.toLocaleDateString(undefined, {month: 'short', day: 'numeric'})}<span class="num">${d.toLocaleTimeString(undefined, {hour: 'numeric', minute: '2-digit'})}</span></a></li>`;
    }).join('') : '<li class="empty">None yet.</li>';
  } catch {}
}

// ---------- purple pixel fade (pixel-id-maker's background, drawn live) ----------
function pixelFade(canvas, cell, from, seed = 7) {
  const r = canvas.getBoundingClientRect(); if (!r.width) return;
  const cols = Math.ceil(r.width / cell), rows = Math.ceil(r.height / cell);
  drawFade(canvas, cols, rows, from, seed);
  canvas.style.imageRendering = 'pixelated';
}
function drawFade(canvas, cols, rows, from, seed = 7) {
  canvas.width = cols; canvas.height = rows;
  const ctx = canvas.getContext('2d'), img = ctx.createImageData(cols, rows);
  const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const ramp = from === 'light'
    ? [[255,255,255],[244,238,252],[217,190,255],[185,140,255],[166,107,255],[124,58,237]]
    : [[20,17,28],[44,30,78],[90,60,170],[124,58,237],[166,107,255],[217,190,255]];
  for (let y = 0; y < rows; y++) for (let x = 0; x < cols; x++) {
    const d = from === 'tr' ? Math.hypot((cols - x) / cols, y / rows * .8) : Math.hypot((cols - x) / cols, (rows - y) / rows);
    const t = Math.max(0, Math.min(1, 1.15 - d * 1.05 + (rnd() - .5) * .35));
    const k = t * (ramp.length - 1), i = Math.floor(k), f = k - i, a = ramp[i], b = ramp[Math.min(i + 1, ramp.length - 1)];
    const o = (y * cols + x) * 4;
    for (let c = 0; c < 3; c++) img.data[o + c] = a[c] + (b[c] - a[c]) * f;
    img.data[o + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
}
const paintFades = () => { pixelFade($('brandFade'), 10, 'tr'); if (!$('empty').hidden) pixelFade($('fade'), 22, 'br'); };

addEventListener('click', e => document.querySelectorAll('details.menu[open]').forEach(m => { if (!m.contains(e.target) || e.target.closest('.menu-list a, .menu-list button')) m.open = false; }));

// ---------- boot ----------
async function boot() {
  loadSettings(); applyTool(); App.view = segVal('viewSeg') || 'single';
  try {
    App.caps = await (await fetch('/api/capabilities')).json();
    $('version').textContent = 'POST PREP v' + App.caps.version;
  } catch {}
  $('upAi').hidden = !App.caps.ai_upscale;
  $('dest').hidden = !App.caps.local; $('outPick').hidden = !App.caps.can_choose;
  $('outDir').placeholder = App.caps.default_dir ? App.caps.default_dir.replace(/^\/Users\/[^/]+/, '~') : '';
  if (!App.caps.ai_upscale && segVal('upSeg') === 'ai') setSeg('upSeg', 'lanczos');
  $('formats').textContent = 'JPEG · PNG · WebP · TIFF · BMP · GIF' + (App.caps.heic ? ' · HEIC · AVIF' : '');
  changed(); loadHistory();
  document.fonts.ready.then(() => { paintFades(); render(); });
}
addEventListener('resize', () => { paintFades(); renderSoon(); });
