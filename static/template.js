// Template tool: frame every photo on a canvas with title, credit, logo and
// watermark. The browser draws the background and overlay layers with the
// same code it previews with; the server only places the photo (src/template.rs).
(() => {
const imgs = {logo: null, bg: null, frame: null}; // {url, img}
const frameMode = () => segVal('tplMode') === 'frame';
const IMG_KEY = 'postprep.tplimg', LIB_KEY = 'postprep.templates';

// ---------- controls ----------
const pos = $('logoPos');
pos.insertAdjacentHTML('afterend', '<input type="hidden" id="logoPosVal" value="8">');
pos.innerHTML = Array.from({length: 9}, (_, i) => `<button type="button" data-p="${i}" aria-label="Logo position ${i + 1}"></button>`).join('');
pos.addEventListener('click', e => { const b = e.target.closest('[data-p]'); if (b) { $('logoPosVal').value = b.dataset.p; changed(); } });

$('canvasSeg').addEventListener('seg', () => {
  const c = segVal('canvasSeg'); if (c === 'custom') return;
  [$('cw').value, $('ch').value] = c.split('x');
});
['cw', 'ch'].forEach(id => $(id).addEventListener('input', matchCanvasSeg));
$('flip').addEventListener('click', () => { [$('cw').value, $('ch').value] = [$('ch').value, $('cw').value]; matchCanvasSeg(); changed(); });
function matchCanvasSeg() {
  const v = `${$('cw').value}x${$('ch').value}`;
  setSeg('canvasSeg', [...$('canvasSeg').querySelectorAll('[data-c]')].some(b => b.dataset.c === v) ? v : 'custom');
}

// Logo and background images are kept as data URLs so templates can carry them.
function pickImage(input, kind, maxSide, type) {
  input.addEventListener('change', async () => {
    const f = input.files[0]; input.value = ''; if (!f) return;
    const img = await loadImg(URL.createObjectURL(f));
    const s = Math.min(1, maxSide / Math.max(img.naturalWidth, img.naturalHeight));
    const c = document.createElement('canvas'); c.width = Math.round(img.naturalWidth * s); c.height = Math.round(img.naturalHeight * s);
    c.getContext('2d').drawImage(img, 0, 0, c.width, c.height);
    setImg(kind, c.toDataURL(type, 0.9)); changed();
  });
}
$('framePick').addEventListener('click', () => $('frameFile').click());
$('frameClear').addEventListener('click', () => { setImg('frame', null); changed(); });
// The frame keeps its original bytes, so soft edges and colors reach the export untouched.
$('frameFile').addEventListener('change', async () => {
  const f = $('frameFile').files[0]; $('frameFile').value = ''; if (!f) return;
  if (f.size > 40e6) return status('That frame is over 40 MB. Export it smaller and try again.', 'err');
  const url = await new Promise((ok, fail) => { const r = new FileReader(); r.onload = () => ok(r.result); r.onerror = fail; r.readAsDataURL(f); });
  try { await setImg('frame', url); } catch { status(`Couldn't read ${f.name} as an image.`, 'err'); }
});
pickImage($('logoFile'), 'logo', 1200, 'image/png');
pickImage($('bgFile'), 'bg', 2400, 'image/jpeg');
$('logoPick').addEventListener('click', () => $('logoFile').click());
$('bgPick').addEventListener('click', () => $('bgFile').click());
$('logoClear').addEventListener('click', () => { setImg('logo', null); changed(); });
const loadImg = src => new Promise((ok, fail) => { const i = new Image(); i.onload = () => ok(i); i.onerror = fail; i.src = src; });
async function setImg(kind, url, persist = true) {
  imgs[kind] = url ? {url, img: await loadImg(url)} : null;
  const thumb = $({logo: 'logoThumb', bg: 'bgThumb', frame: 'frameThumb'}[kind]);
  thumb.hidden = !url; if (url) thumb.src = url;
  if (persist) store.set(IMG_KEY, {logo: imgs.logo?.url || null, bg: imgs.bg?.url || null, frame: imgs.frame?.url || null}) || status('Image too large to remember after reload, but it will still export.', 'err');
  changed();
}

// ---------- layout ----------
function layout() {
  if (frameMode()) {
    const f = imgs.frame?.img, W = f ? Math.min(8000, Math.max(16, f.naturalWidth)) : 1080, H = f ? Math.min(8000, Math.max(16, f.naturalHeight)) : 1350;
    return {
      w: W, h: H, slot: {x: 0, y: 0, w: W, h: H}, cover: segVal('frFit') === 'cover', border: 0, border_color: '#ffffff',
      radius: 0, shadow: false, blur_bg: segVal('frBg') === 'blur', bg_color: $('frColor').value,
    };
  }
  const W = Math.min(8000, Math.max(200, +$('cw').value || 1080)), H = Math.min(8000, Math.max(200, +$('ch').value || 1440));
  // Padding goes round all four sides; text space is added only where there is text.
  const pad = Math.round(Math.min(W, H) * $('padAmt').value / 100), gap = Math.round(H * $('textSpace').value / 100);
  const ms = pad, mt = pad + ($('title').value.trim() ? gap : 0), mb = pad + ($('sub').value.trim() ? gap : 0);
  return {
    w: W, h: H, slot: {x: ms, y: mt, w: Math.max(16, W - 2 * ms), h: Math.max(16, H - mt - mb)},
    cover: segVal('fitSeg') === 'cover', border: +$('border').value, border_color: $('borderColor').value,
    radius: +$('radius').value, shadow: $('shadow').checked, blur_bg: segVal('bgSeg') === 'blur', bg_color: $('bgColor').value,
  };
}

// Where the photo lands inside the slot; mirrors template::compose.
function placement(p, L) {
  const s = L.slot, up = segVal('upSeg') !== 'off';
  if (L.cover) {
    const [cw, ch] = p.w * s.h > p.h * s.w ? [Math.floor(p.h * s.w / s.h), p.h] : [p.w, Math.floor(p.w * s.h / s.w)];
    const factor = s.w / cw;
    return {src: [Math.floor((p.w - cw) / 2), Math.floor((p.h - ch) / 2), cw, ch], x: s.x, y: s.y, w: s.w, h: s.h, factor, native: false};
  }
  const k = Math.min(s.w / p.w, s.h / p.h), native = k > 1 && !up;
  const w = native ? p.w : Math.max(1, Math.round(p.w * k)), h = native ? p.h : Math.max(1, Math.round(p.h * k));
  return {src: [0, 0, p.w, p.h], x: s.x + Math.trunc((s.w - w) / 2), y: s.y + Math.trunc((s.h - h) / 2), w, h, factor: k, native};
}

// ---------- drawing (full-resolution coordinates; callers scale the context) ----------
function drawUnder(ctx, L, p) {
  const {w: W, h: H} = L, fm = frameMode(), bg = fm ? (L.blur_bg ? 'blur' : 'solid') : segVal('bgSeg');
  ctx.fillStyle = fm ? $('frColor').value : $('bgColor').value; ctx.fillRect(0, 0, W, H);
  if (bg === 'fade') {
    const c = document.createElement('canvas'); drawFade(c, Math.ceil(W / 36), Math.ceil(H / 36), 'light', 11);
    ctx.imageSmoothingEnabled = false; ctx.drawImage(c, 0, 0, Math.ceil(W / 36) * 36, Math.ceil(H / 36) * 36); ctx.imageSmoothingEnabled = true;
  } else if (bg === 'image' && imgs.bg) {
    const i = imgs.bg.img, k = Math.max(W / i.naturalWidth, H / i.naturalHeight);
    ctx.drawImage(i, (W - i.naturalWidth * k) / 2, (H - i.naturalHeight * k) / 2, i.naturalWidth * k, i.naturalHeight * k);
  } else if (bg === 'blur' && p) {
    // Preview only: the server blurs the real photo.
    const k = Math.max(W / p.w, H / p.h);
    ctx.save(); ctx.filter = `blur(${Math.round(W / 45)}px) brightness(.75)`;
    drawPhoto(ctx, p, 0, 0, p.w, p.h, (W - p.w * k) / 2 - W * .05, (H - p.h * k) / 2 - H * .05, p.w * k * 1.1, p.h * k * 1.1);
    ctx.restore();
  }
}
function drawPlaced(ctx, L, p) {
  const r = placement(p, L), rad = Math.min(L.radius, r.w / 2, r.h / 2);
  if (L.shadow) {
    const spread = Math.max(6, Math.max(r.w, r.h) / 25);
    ctx.save(); ctx.shadowColor = 'rgba(0,0,0,.47)'; ctx.shadowBlur = spread * 1.4; ctx.shadowOffsetY = spread / 2;
    ctx.fillStyle = '#000'; ctx.beginPath(); ctx.roundRect(r.x, r.y, r.w, r.h, rad); ctx.fill(); ctx.restore();
  }
  if (L.border) {
    ctx.fillStyle = L.border_color; ctx.beginPath();
    ctx.roundRect(r.x - L.border, r.y - L.border, r.w + 2 * L.border, r.h + 2 * L.border, rad ? rad + L.border : 0); ctx.fill();
  }
  ctx.save(); ctx.beginPath(); ctx.roundRect(r.x, r.y, r.w, r.h, rad); ctx.clip();
  drawPhoto(ctx, p, ...r.src, r.x, r.y, r.w, r.h);
  ctx.restore();
  return r;
}
function drawOver(ctx, L) {
  if (frameMode()) { if (imgs.frame) ctx.drawImage(imgs.frame.img, 0, 0, L.w, L.h); return; }
  const {w: W, h: H, slot: s} = L, align = segVal('alignSeg'), font = $('font').value;
  const x = align === 'left' ? s.x : align === 'right' ? s.x + s.w : W / 2;
  ctx.fillStyle = $('textColor').value; ctx.textAlign = align; ctx.textBaseline = 'middle';
  const weight = font === 'Silkscreen' ? 700 : 600;
  const text = (t, size, y) => { if (!t.trim()) return; ctx.font = `${weight} ${size}px ${font.includes(',') ? font : `"${font}"`}`; ctx.fillText(t, x, y, s.w); };
  const ts = +$('textSize').value;
  text($('title').value, ts, s.y / 2);
  text($('sub').value, Math.round(ts * .46), s.y + s.h + (H - s.y - s.h) / 2);

  if (imgs.logo) {
    const i = imgs.logo.img, lw = W * $('logoSize').value / 100, lh = lw * i.naturalHeight / i.naturalWidth;
    const m = Math.max(W * .03, Math.min(s.x, s.y) * .25), p = +$('logoPosVal').value, c = p % 3, row = Math.floor(p / 3);
    ctx.globalAlpha = $('logoAlpha').value / 100;
    ctx.drawImage(i, c === 0 ? m : c === 1 ? (W - lw) / 2 : W - m - lw, row === 0 ? m : row === 1 ? (H - lh) / 2 : H - m - lh, lw, lh);
    ctx.globalAlpha = 1;
  }

  const wm = segVal('wmSeg');
  if (wm === 'off') return;
  const useLogo = $('wmLogo').checked && imgs.logo, size = W * $('wmSize').value / 100;
  ctx.save(); ctx.beginPath(); ctx.rect(s.x, s.y, s.w, s.h); ctx.clip();
  ctx.globalAlpha = $('wmAlpha').value / 100; ctx.fillStyle = $('wmColor').value; ctx.textBaseline = 'middle'; ctx.textAlign = 'center';
  ctx.font = `600 ${size}px "Space Grotesk"`;
  const mark = (cx, cy) => {
    if (useLogo) { const i = imgs.logo.img, lw = size * 4, lh = lw * i.naturalHeight / i.naturalWidth; ctx.drawImage(i, cx - lw / 2, cy - lh / 2, lw, lh); }
    else ctx.fillText($('wmText').value, cx, cy);
  };
  const mw = useLogo ? size * 4 : ctx.measureText($('wmText').value).width;
  if (wm === 'corner') mark(s.x + s.w - mw / 2 - size * .8, s.y + s.h - size * 1.2);
  else if (wm === 'center') mark(s.x + s.w / 2, s.y + s.h / 2);
  else {
    ctx.translate(s.x + s.w / 2, s.y + s.h / 2); ctx.rotate(-Math.PI / 6);
    const stepX = mw + size * 3, stepY = size * 4, span = Math.hypot(s.w, s.h);
    for (let y = -span / 2, r = 0; y < span / 2; y += stepY, r++) for (let x = -span / 2 + (r % 2) * stepX / 2; x < span / 2; x += stepX) mark(x, y);
  }
  ctx.restore();
}
function drawAll(ctx, L, p) {
  drawUnder(ctx, L, p);
  const r = p?.ready ? drawPlaced(ctx, L, p) : null;
  drawOver(ctx, L);
  return r;
}

// ---------- views ----------
let single = null, lastPhoto = null, gridToken = 0;
function render() {
  const view = $('tplView'), L = layout(), p = App.photo;
  if (App.view === 'grid') return renderGrid(view, L);
  if (!single || !view.contains(single.wrap)) {
    view.innerHTML = `<div class="tpl-single"><div><canvas></canvas><div class="readout num"></div></div></div>`;
    single = {wrap: view.firstElementChild, canvas: view.querySelector('canvas'), read: view.querySelector('.readout')};
  }
  const st = $('stage'), dpr = devicePixelRatio || 1;
  const k = Math.min((st.clientWidth - 64) / L.w, (st.clientHeight - 96) / L.h, 1);
  const c = single.canvas;
  c.width = Math.round(L.w * k * dpr); c.height = Math.round(L.h * k * dpr);
  c.style.width = `${Math.round(L.w * k)}px`; c.style.height = `${Math.round(L.h * k)}px`;
  if (p !== lastPhoto) { c.style.animation = 'none'; c.offsetWidth; c.style.animation = ''; lastPhoto = p; }
  const ctx = c.getContext('2d'); ctx.setTransform(k * dpr, 0, 0, k * dpr, 0, 0);
  const r = drawAll(ctx, L, p);
  single.read.innerHTML = `<span><b>${L.w}×${L.h}</b></span>`
    + (r && r.factor > 1 ? (r.native ? '<span class="badge warn">PHOTO TOO SMALL, KEPT NATIVE</span>' : `<span class="badge">UPSCALED ×${r.factor.toFixed(1)}</span>`) : '');
}

async function renderGrid(view, L) {
  single = null;
  const token = ++gridToken;
  view.innerHTML = `<div class="view"><section class="block"><h3>All photos <span class="num">${App.photos.length}</span></h3><div class="tiles" id="tplTiles"></div></section></div>`;
  const tiles = view.querySelector('#tplTiles'), k = Math.min(240 / L.h, 200 / L.w);
  for (let i = 0; i < App.photos.length; i++) {
    if (i % 6 === 5) { await new Promise(requestAnimationFrame); if (token !== gridToken) return; }
    const p = App.photos[i], c = document.createElement('canvas'), dpr = devicePixelRatio || 1;
    c.width = Math.round(L.w * k * dpr); c.height = Math.round(L.h * k * dpr); c.style.width = `${Math.round(L.w * k)}px`;
    const ctx = c.getContext('2d'); ctx.setTransform(k * dpr, 0, 0, k * dpr, 0, 0); drawAll(ctx, L, p);
    const fig = document.createElement('figure'); fig.className = 'tile pick' + (i === App.current ? ' cur' : ''); fig.style.setProperty('--i', Math.min(i, 12));
    fig.append(c); fig.insertAdjacentHTML('beforeend', `<figcaption><span>${esc(p.file.name)}</span></figcaption>`);
    fig.addEventListener('click', () => { App.current = i; App.view = 'single'; setSeg('viewSeg', 'single'); changed(); });
    tiles.append(fig);
  }
}

// ---------- export payload ----------
async function payload() {
  const L = layout(), toBlob = c => new Promise(r => c.toBlob(r, 'image/png'));
  const layer = draw => { const c = document.createElement('canvas'); c.width = L.w; c.height = L.h; draw(c.getContext('2d')); return toBlob(c); };
  if (frameMode()) {
    if (!imgs.frame) throw new Error('choose a PNG frame first');
    return {layout: L, under: null, over: await (await fetch(imgs.frame.url)).blob()};
  }
  const bg = segVal('bgSeg');
  return {
    layout: L,
    under: bg === 'fade' || (bg === 'image' && imgs.bg) ? await layer(ctx => drawUnder(ctx, L, null)) : null,
    over: await layer(ctx => drawOver(ctx, L)),
  };
}

// ---------- library ----------
const TPL_SCOPE = () => document.querySelectorAll('#tool-template input[id], #tool-template select[id]');
function snapshot(name) {
  const t = {name, version: 1, inputs: {}, segs: {}, logo: imgs.logo?.url || null, bg: imgs.bg?.url || null, frame: imgs.frame?.url || null};
  TPL_SCOPE().forEach(i => { if (i.type !== 'file' && i.id !== 'tplPick') t.inputs[i.id] = i.type === 'checkbox' ? i.checked : i.value; });
  document.querySelectorAll('#tool-template .seg[id]').forEach(s => t.segs[s.id] = segVal(s.id));
  return t;
}
async function apply(t) {
  for (const [id, v] of Object.entries(t.inputs || {})) { const i = $(id); if (!i) continue; if (i.type === 'checkbox') i.checked = v; else i.value = v; }
  for (const [id, v] of Object.entries(t.segs || {})) if ($(id)) setSeg(id, v);
  await setImg('logo', t.logo); await setImg('bg', t.bg); await setImg('frame', t.frame || null);
  App.tplName = t.name; changed();
}
const lib = () => store.get(LIB_KEY) || {};
function renderLib() {
  const names = Object.keys(lib()).sort();
  $('tplPick').innerHTML = `<option value="">${App.tplName ? 'Unsaved changes' : 'Unsaved template'}</option>` + names.map(n => `<option ${n === App.tplName ? 'selected' : ''}>${esc(n)}</option>`).join('');
  $('libDelete').disabled = !App.tplName;
}
$('tplPick').addEventListener('change', async () => {
  const n = $('tplPick').value; if (!n) return;
  await apply(lib()[n]); renderLib(); status(`Loaded "${n}".`);
});
$('libSave').addEventListener('click', () => {
  const all = lib(), name = (prompt('Save template as', App.tplName || `Template ${Object.keys(all).length + 1}`) || '').trim();
  if (!name) return;
  all[name] = snapshot(name);
  if (!store.set(LIB_KEY, all)) return status('Browser storage is full; delete a template or use a smaller background image.', 'err');
  App.tplName = name; renderLib(); status(`Saved "${name}".`, 'ok');
});
$('libDelete').addEventListener('click', () => {
  const n = App.tplName; if (!n || !confirm(`Delete "${n}"?`)) return;
  const all = lib(); delete all[n]; store.set(LIB_KEY, all); App.tplName = null; renderLib(); status(`Deleted "${n}".`);
});
$('libExport').addEventListener('click', () => {
  const t = snapshot(App.tplName || 'template'), a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([JSON.stringify(t, null, 2)], {type: 'application/json'}));
  a.download = `${stemOf(t.name)}.postprep.json`; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 1000);
});
$('libImport').addEventListener('click', () => $('libFile').click());
$('libFile').addEventListener('change', async () => {
  const f = $('libFile').files[0]; $('libFile').value = ''; if (!f) return;
  try {
    const t = JSON.parse(await f.text());
    if (!t.inputs) throw new Error('not a Post Prep template');
    t.name = t.name || stemOf(f.name);
    const all = lib(); all[t.name] = t; store.set(LIB_KEY, all);
    await apply(t); renderLib(); status(`Imported "${t.name}".`, 'ok');
  } catch (err) { status(`Couldn't import ${f.name}: ${err.message}.`, 'err'); }
});

// ---------- labels ----------
function sync() {
  const L = layout(), bg = segVal('bgSeg'), wm = segVal('wmSeg'), fm = frameMode();
  document.querySelectorAll('#tool-template details[name=tpl]').forEach(d => { d.hidden = d.id === 'frameSec' ? !fm : fm; });
  if (fm && ![...document.querySelectorAll('#tool-template details[name=tpl]')].some(d => d.open && !d.hidden)) $('frameSec').open = true;
  if (!fm && ![...document.querySelectorAll('#tool-template details[name=tpl]')].some(d => d.open && !d.hidden)) $('layoutSec').open = true;
  $('frameTag').textContent = imgs.frame ? `${L.w}×${L.h}` : '';
  $('frameClear').hidden = !imgs.frame;
  $('frBgRow').hidden = segVal('frFit') !== 'contain';
  $('frColor').hidden = segVal('frBg') === 'blur';
  $('canvasTag').textContent = `${L.w}×${L.h}`;
  $('customSize').hidden = segVal('canvasSeg') !== 'custom';
  $('logoFields').hidden = $('logoClear').hidden = !imgs.logo;
  $('bgSolid').hidden = bg !== 'solid' && bg !== 'fade';
  $('bgImageRow').hidden = bg !== 'image';
  $('wmFields').hidden = wm === 'off';
  $('wmTag').textContent = wm === 'off' ? 'Off' : wm[0].toUpperCase() + wm.slice(1);
  $('logoTag').textContent = imgs.logo ? 'Logo' : '';
  pos.querySelectorAll('[data-p]').forEach(b => b.classList.toggle('on', b.dataset.p === $('logoPosVal').value));
}

App.tools.template = {label: 'into frames', render, payload, sync};
const saved = store.get(IMG_KEY);
Promise.all([saved?.logo && setImg('logo', saved.logo, false), saved?.bg && setImg('bg', saved.bg, false), saved?.frame && setImg('frame', saved.frame, false)]).finally(() => { renderLib(); boot(); });
})();
