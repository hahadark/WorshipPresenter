'use strict';
// .pptx/.ppsx(OOXML) 를 PowerPoint 없이 직접 읽어 슬라이드의 요소를 꺼낸다 (PPT 요소 따로 고치기 · 슬라이드 그림 만들기)
//   - 글상자: 위치·크기 · 글(문단·글 조각) · 글자 크기·색·굵게·기울임·글꼴 · 정렬 · 여백 · 줄 간격 · 글머리표
//     레이아웃·마스터 개체 틀(placeholder) · 마스터 글 스타일(titleStyle·bodyStyle·otherStyle) · 테마 색·글꼴을 따라 올라가며 채운다
//   - 그림(ppt/media, 자르기 srcRect) · 도형(미리 정의된 모양 · 채우기 · 선) · 그룹(좌표 변환) · 표 · SmartArt(그려 둔 도형) · 배경(슬라이드 → 레이아웃 → 마스터)
//   - 그림으로 그리는 일은 pptxsvg.js, 편집기 요소로 바꾸는 일은 pptxlayers.js 가 한다
// 직접 쓴 코드 (외부 라이브러리 없음). 이음오피스(Apache-2.0)·FreeShow(GPL)·pptxtojson(MIT) 의 구조를 참고했을 뿐 코드는 가져오지 않았다.

const fs = require('fs');
const { readZip } = require('./pptrender');

const EMU_PT = 12700; // 1pt = 12700 EMU

// ---------------- 작은 XML 파서 (OOXML 은 늘 잘 짜인 XML 이라 이것으로 충분) ----------------
const ENT = { lt: '<', gt: '>', amp: '&', quot: '"', apos: "'" };
function decode(s) {
  if (s.indexOf('&') < 0) return s;
  return s.replace(/&(#x[0-9a-fA-F]+|#\d+|[a-zA-Z]+);/g, (m, e) => {
    if (e[0] === '#') { const n = e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10); return Number.isFinite(n) && n > 0 && n < 0x110000 ? String.fromCodePoint(n) : ''; }
    return ENT[e] ?? m;
  });
}
/** XML 글 → 노드 { n: 이름, ln: 접두사 뺀 이름, a: {속성}, c: [자식], t: 글(글 노드) } */
function parseXml(str) {
  const root = { n: '#root', ln: '#root', a: {}, c: [] };
  const stack = [root];
  let i = 0;
  const len = str.length;
  while (i < len) {
    const lt = str.indexOf('<', i);
    if (lt < 0) break;
    if (lt > i) { const t = str.slice(i, lt); if (t.trim()) stack[stack.length - 1].c.push({ n: '#text', ln: '#text', a: {}, c: [], t: decode(t) }); else if (stack.length > 1 && stack[stack.length - 1].ln === 't') stack[stack.length - 1].c.push({ n: '#text', ln: '#text', a: {}, c: [], t }); }
    if (str.startsWith('<?', lt)) { const e = str.indexOf('?>', lt); i = e < 0 ? len : e + 2; continue; }
    if (str.startsWith('<!--', lt)) { const e = str.indexOf('-->', lt); i = e < 0 ? len : e + 3; continue; }
    if (str.startsWith('<![CDATA[', lt)) { const e = str.indexOf(']]>', lt); const t = str.slice(lt + 9, e < 0 ? len : e); stack[stack.length - 1].c.push({ n: '#text', ln: '#text', a: {}, c: [], t }); i = e < 0 ? len : e + 3; continue; }
    if (str.startsWith('<!', lt)) { const e = str.indexOf('>', lt); i = e < 0 ? len : e + 1; continue; }
    const gt = findTagEnd(str, lt);
    if (gt < 0) break;
    const inner = str.slice(lt + 1, gt);
    if (inner[0] === '/') { if (stack.length > 1) stack.pop(); i = gt + 1; continue; }
    const selfClose = inner.endsWith('/');
    const body = selfClose ? inner.slice(0, -1) : inner;
    let p = 0;
    while (p < body.length && !/\s/.test(body[p])) p++;
    const name = body.slice(0, p);
    const node = { n: name, ln: name.slice(name.indexOf(':') + 1), a: {}, c: [] };
    const re = /([^\s=]+)\s*=\s*("([^"]*)"|'([^']*)')/g;
    re.lastIndex = p;
    let m;
    while ((m = re.exec(body))) node.a[m[1]] = decode(m[3] ?? m[4] ?? '');
    stack[stack.length - 1].c.push(node);
    if (!selfClose) stack.push(node);
    i = gt + 1;
  }
  return root.c.find(x => x.n !== '#text') || root;
}
function findTagEnd(str, lt) {
  let q = '';
  for (let k = lt + 1; k < str.length; k++) {
    const ch = str[k];
    if (q) { if (ch === q) q = ''; continue; }
    if (ch === '"' || ch === "'") q = ch;
    else if (ch === '>') return k;
  }
  return -1;
}
/** 접두사 뺀 이름으로 첫 자식 */
const kid = (node, ln) => (node ? node.c.find(x => x.ln === ln) : undefined);
const kids = (node, ln) => (node ? node.c.filter(x => x.ln === ln) : []);
/** 깊이 상관없이 첫 후손 */
function find(node, ln) {
  if (!node) return undefined;
  for (const c of node.c) { if (c.ln === ln) return c; const r = find(c, ln); if (r) return r; }
  return undefined;
}
const attr = (node, name, d = '') => (node && node.a[name] != null ? node.a[name] : d);
const num = (v, d = 0) => { if (v === '' || v == null) return d; const n = Number(v); return Number.isFinite(n) ? n : d; };
/** 노드 아래의 글 전부 */
function textOf(node) { if (!node) return ''; if (node.n === '#text') return node.t; return node.c.map(textOf).join(''); }

// ---------------- 패키지 (zip · 관계) ----------------
const RT = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/';
function joinPath(from, target) {
  if (target.startsWith('/')) return target.slice(1);
  const parts = from.split('/').slice(0, -1);
  for (const seg of target.split('/')) {
    if (seg === '..') parts.pop();
    else if (seg !== '.' && seg !== '') parts.push(seg);
  }
  return parts.join('/');
}
class Pkg {
  constructor(buf) {
    this.zip = readZip(buf);
    this.names = new Set(this.zip.names);
    this.xmlCache = new Map();
    this.relCache = new Map();
  }
  has(name) { return this.names.has(name); }
  raw(name) { return this.zip.get(name); }
  xml(name) {
    if (this.xmlCache.has(name)) return this.xmlCache.get(name);
    const b = this.zip.get(name);
    const x = b ? parseXml(b.toString('utf8')) : null;
    this.xmlCache.set(name, x);
    return x;
  }
  /** 부분(part)의 관계: Map(rId → { type(짧게), target(절대 경로), external }) */
  rels(part) {
    if (this.relCache.has(part)) return this.relCache.get(part);
    const i = part.lastIndexOf('/');
    const relName = part.slice(0, i + 1) + '_rels/' + part.slice(i + 1) + '.rels';
    const map = new Map();
    const x = this.has(relName) ? this.xml(relName) : null;
    if (x) for (const r of kids(x, 'Relationship')) {
      const external = attr(r, 'TargetMode') === 'External';
      map.set(attr(r, 'Id'), { type: attr(r, 'Type').replace(RT, '').replace(/^.*\//, ''), target: external ? attr(r, 'Target') : joinPath(part, attr(r, 'Target')), external });
    }
    this.relCache.set(part, map);
    return map;
  }
  relTarget(part, rId) { const r = this.rels(part).get(rId); return r && !r.external ? r.target : null; }
  relByType(part, type) { for (const r of this.rels(part).values()) if (r.type === type && !r.external) return r.target; return null; }
}

// ---------------- 색 ----------------
const PRST_CLR = { black: '000000', white: 'FFFFFF', red: 'FF0000', green: '008000', blue: '0000FF', yellow: 'FFFF00', gray: '808080', grey: '808080', darkGray: 'A9A9A9', lightGray: 'D3D3D3', orange: 'FFA500', purple: '800080', navy: '000080', silver: 'C0C0C0', maroon: '800000', olive: '808000', teal: '008080', aqua: '00FFFF', cyan: '00FFFF', magenta: 'FF00FF', fuchsia: 'FF00FF', lime: '00FF00', gold: 'FFD700', pink: 'FFC0CB', brown: 'A52A2A', darkBlue: '00008B', darkRed: '8B0000', darkGreen: '006400', lightBlue: 'ADD8E6', ltGray: 'D3D3D3', dkGray: 'A9A9A9', medGray: '808080' };
const hex2 = v => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0');
function rgbToHsl(r, g, b) {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b), l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min, s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h / 6, s, l];
}
function hslToRgb(h, s, l) {
  if (s === 0) return [l * 255, l * 255, l * 255];
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s, p = 2 * l - q;
  const f = t => { t = (t + 1) % 1; if (t < 1 / 6) return p + (q - p) * 6 * t; if (t < 1 / 2) return q; if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6; return p; };
  return [f(h + 1 / 3) * 255, f(h) * 255, f(h - 1 / 3) * 255];
}
/** 색 노드(srgbClr·schemeClr·sysClr·prstClr·scrgbClr·hslClr) → { hex:'#rrggbb', alpha } — 밝기·틴트·투명 보정 포함 */
function resolveColor(node, ctx, phClr) {
  if (!node) return null;
  let hex = null;
  const v = attr(node, 'val');
  switch (node.ln) {
    case 'srgbClr': hex = v; break;
    case 'sysClr': hex = attr(node, 'lastClr') || (v === 'window' ? 'FFFFFF' : '000000'); break;
    case 'prstClr': hex = PRST_CLR[v] || '000000'; break;
    case 'scrgbClr': hex = ['r', 'g', 'b'].map(k => hex2(num(attr(node, k)) / 100000 * 255)).join(''); break;
    case 'hslClr': { const [r, g, b] = hslToRgb(num(attr(node, 'hue')) / 21600000, num(attr(node, 'sat')) / 100000, num(attr(node, 'lum')) / 100000); hex = hex2(r) + hex2(g) + hex2(b); break; }
    case 'schemeClr': {
      if (v === 'phClr') hex = phClr ? phClr.replace('#', '') : null;
      else {
        const key = (ctx && ctx.clrMap && ctx.clrMap[v]) || v;
        hex = ctx && ctx.theme && ctx.theme.colors[key] ? ctx.theme.colors[key] : ctx && ctx.theme && ctx.theme.colors[v];
      }
      break;
    }
    default: return null;
  }
  if (!hex || !/^[0-9a-fA-F]{6}$/.test(hex)) return null;
  let r = parseInt(hex.slice(0, 2), 16), g = parseInt(hex.slice(2, 4), 16), b = parseInt(hex.slice(4, 6), 16);
  let alpha = 1;
  for (const m of node.c) {
    const val = num(attr(m, 'val')) / 100000;
    if (m.ln === 'alpha') alpha = Math.max(0, Math.min(1, val));
    else if (m.ln === 'tint') { r = 255 - (255 - r) * val; g = 255 - (255 - g) * val; b = 255 - (255 - b) * val; } // 흰색 쪽으로
    else if (m.ln === 'shade') { r *= val; g *= val; b *= val; }
    else if (m.ln === 'lumMod' || m.ln === 'lumOff' || m.ln === 'satMod' || m.ln === 'hueMod') {
      let [h, s, l] = rgbToHsl(r, g, b);
      if (m.ln === 'lumMod') l *= val; else if (m.ln === 'lumOff') l += val; else if (m.ln === 'satMod') s *= val; else h = (h * val) % 1;
      [r, g, b] = hslToRgb(h, Math.max(0, Math.min(1, s)), Math.max(0, Math.min(1, l)));
    } else if (m.ln === 'comp') { [r, g, b] = [255 - r, 255 - g, 255 - b]; }
    else if (m.ln === 'inv') { [r, g, b] = [255 - r, 255 - g, 255 - b]; }
  }
  return { hex: '#' + hex2(r) + hex2(g) + hex2(b), alpha };
}
const COLOR_KIDS = ['srgbClr', 'schemeClr', 'sysClr', 'prstClr', 'scrgbClr', 'hslClr'];
const colorKid = node => (node ? node.c.find(c => COLOR_KIDS.includes(c.ln)) : undefined);

// ---------------- 테마 ----------------
function parseTheme(x) {
  const t = { colors: {}, fonts: { major: { latin: '', ea: '' }, minor: { latin: '', ea: '' } }, fills: [], bgFills: [], lines: [] };
  const te = kid(x, 'themeElements');
  const cs = kid(te, 'clrScheme');
  if (cs) for (const c of cs.c) { const col = resolveColor(colorKid(c), null); if (col) t.colors[c.ln] = col.hex.slice(1); }
  const fsch = kid(te, 'fontScheme');
  const fontOf = n => {
    if (!n) return { latin: '', ea: '' };
    const latin = attr(kid(n, 'latin'), 'typeface');
    let ea = attr(kid(n, 'ea'), 'typeface');
    if (!ea) { const hang = kids(n, 'font').find(f => attr(f, 'script') === 'Hang'); ea = attr(hang, 'typeface'); }
    return { latin, ea };
  };
  if (fsch) { t.fonts.major = fontOf(kid(fsch, 'majorFont')); t.fonts.minor = fontOf(kid(fsch, 'minorFont')); }
  const fmt = kid(te, 'fmtScheme');
  if (fmt) {
    t.fills = (kid(fmt, 'fillStyleLst') || { c: [] }).c;
    t.bgFills = (kid(fmt, 'bgFillStyleLst') || { c: [] }).c;
    t.lines = (kid(fmt, 'lnStyleLst') || { c: [] }).c;
  }
  return t;
}
function parseClrMap(node) {
  const m = {};
  if (node) for (const [k, v] of Object.entries(node.a)) m[k] = v;
  return m;
}
/** 테마 글꼴 이름('+mj-lt' '+mn-ea' …) 풀기 */
function themeFont(name, theme, isEa) {
  if (!name) return '';
  if (name[0] !== '+') return name;
  const m = /^\+(mj|mn)-(lt|ea|cs)$/.exec(name);
  if (!m || !theme) return '';
  const f = theme.fonts[m[1] === 'mj' ? 'major' : 'minor'];
  return (m[2] === 'ea' || isEa ? f.ea : f.latin) || f.latin || '';
}

// ---------------- 채우기 · 선 · 위치 ----------------
/** 채우기 노드 묶음(spPr·bgPr·rPr…)의 채우기 → { type: none|solid|grad|blip|pattern, … } (없으면 null) */
function parseFill(parent, ctx, phClr) {
  if (!parent) return null;
  for (const c of parent.c) {
    if (c.ln === 'noFill') return { type: 'none' };
    if (c.ln === 'solidFill') { const col = resolveColor(colorKid(c), ctx, phClr); return col ? { type: 'solid', color: col.hex, alpha: col.alpha } : { type: 'none' }; }
    if (c.ln === 'gradFill') {
      const stops = kids(kid(c, 'gsLst'), 'gs').map(g => { const col = resolveColor(colorKid(g), ctx, phClr); return col ? { pos: num(attr(g, 'pos')) / 100000, color: col.hex, alpha: col.alpha } : null; }).filter(Boolean).sort((a, b) => a.pos - b.pos);
      const lin = kid(c, 'lin');
      const pth = kid(c, 'path');
      if (!stops.length) return { type: 'none' };
      return { type: 'grad', angle: lin ? num(attr(lin, 'ang')) / 60000 : 90, path: pth ? attr(pth, 'path') : '', stops };
    }
    if (c.ln === 'blipFill') {
      const blip = kid(c, 'blip');
      const rId = attr(blip, 'embed') || attr(blip, 'r:embed');
      const src = kid(c, 'srcRect');
      const crop = src ? { l: num(attr(src, 'l')) / 100000, t: num(attr(src, 't')) / 100000, r: num(attr(src, 'r')) / 100000, b: num(attr(src, 'b')) / 100000 } : null;
      let alpha = 1;
      const am = kid(blip, 'alphaModFix');
      if (am) alpha = num(attr(am, 'amt'), 100000) / 100000;
      return { type: 'blip', rId, crop: crop && (crop.l || crop.t || crop.r || crop.b) ? crop : null, tile: !!kid(c, 'tile'), alpha, part: ctx && ctx.part && rId ? ctx.pkg.relTarget(ctx.part, rId) : null };
    }
    if (c.ln === 'pattFill') {
      const fg = resolveColor(colorKid(kid(c, 'fgClr')), ctx, phClr), bg = resolveColor(colorKid(kid(c, 'bgClr')), ctx, phClr);
      return { type: 'pattern', prst: attr(c, 'prst'), fg: fg ? fg.hex : '#000000', bg: bg ? bg.hex : '#ffffff' };
    }
    if (c.ln === 'grpFill') return { type: 'group' };
  }
  return null;
}
/** 테마 스타일 참조(fillRef·lnRef idx) → 테마 fmtScheme 의 채우기·선 노드 (idx 1001~ 은 배경 채우기) */
function themeRefFill(refNode, ctx) {
  if (!refNode || !ctx.theme) return null;
  const idx = num(attr(refNode, 'idx'));
  if (!idx) return null;
  const col = resolveColor(colorKid(refNode), ctx);
  const list = idx >= 1001 ? ctx.theme.bgFills : ctx.theme.fills;
  const node = list[(idx >= 1001 ? idx - 1001 : idx - 1)];
  if (!node) return null;
  return parseFill({ c: [node] }, ctx, col ? col.hex : null);
}
/** 선 → { w(pt), fill, dash } · 선 없음이면 null */
function parseLine(spPr, styleNode, ctx) {
  const ln = kid(spPr, 'ln');
  let fill = ln ? parseFill(ln, ctx) : null;
  let w = ln && attr(ln, 'w') ? num(attr(ln, 'w')) / EMU_PT : null;
  const lnRef = kid(styleNode, 'lnRef');
  if (!fill && lnRef && num(attr(lnRef, 'idx')) > 0 && ctx.theme) {
    const node = ctx.theme.lines[num(attr(lnRef, 'idx')) - 1];
    const col = resolveColor(colorKid(lnRef), ctx);
    if (node) { fill = parseFill(node, ctx, col ? col.hex : null); if (w == null && attr(node, 'w')) w = num(attr(node, 'w')) / EMU_PT; }
  }
  if (!fill || fill.type === 'none') return null;
  if (fill.type !== 'solid') fill = { type: 'solid', color: fill.stops ? fill.stops[0].color : fill.fg || '#000000', alpha: 1 };
  const dash = attr(kid(ln, 'prstDash'), 'val');
  const headEnd = kid(ln, 'headEnd'), tailEnd = kid(ln, 'tailEnd');
  return { w: w == null ? 0.75 : w, color: fill.color, alpha: fill.alpha, dash: dash && dash !== 'solid' ? dash : '', head: headEnd ? attr(headEnd, 'type') : 'none', tail: tailEnd ? attr(tailEnd, 'type') : 'none' };
}
/** 위치·크기 (EMU) · 회전(도) · 뒤집기 */
function parseXfrm(node) {
  const x = kid(node, 'xfrm');
  if (!x) return null;
  const off = kid(x, 'off'), ext = kid(x, 'ext');
  const chOff = kid(x, 'chOff'), chExt = kid(x, 'chExt');
  return {
    x: num(attr(off, 'x')), y: num(attr(off, 'y')), w: num(attr(ext, 'cx')), h: num(attr(ext, 'cy')),
    rot: num(attr(x, 'rot')) / 60000, flipH: attr(x, 'flipH') === '1', flipV: attr(x, 'flipV') === '1',
    ch: chOff && chExt ? { x: num(attr(chOff, 'x')), y: num(attr(chOff, 'y')), w: num(attr(chExt, 'cx')), h: num(attr(chExt, 'cy')) } : null,
    present: !!(off && ext),
  };
}

// ---------------- 글 ----------------
const BODY_PH_TYPES = ['body', 'subTitle', 'obj', 'tbl', 'chart', 'clipArt', 'dgm', 'media', 'pic', 'sldImg', 'hdr'];
function phStyleKind(ph) {
  if (!ph) return 'other';
  if (ph.type === 'title' || ph.type === 'ctrTitle') return 'title';
  if (!ph.type && ph.idx === '0') return 'title';
  if (BODY_PH_TYPES.includes(ph.type) || (!ph.type && ph.idx)) return 'body';
  return 'other';
}
/** lstStyle(또는 txStyles 의 titleStyle 등) → [lvl1pPr, lvl2pPr, …] (0부터) */
function levels(node) {
  const out = [];
  if (node) for (const c of node.c) { const m = /^lvl(\d)pPr$/.exec(c.ln); if (m) out[+m[1] - 1] = c; }
  return out;
}
/** 글 조각 속성 한 겹 (rPr·defRPr) → { sz, b, i, u, color, latin, ea } (있는 것만) */
function runLayer(rPr, ctx) {
  if (!rPr) return null;
  const o = {};
  if (attr(rPr, 'sz')) o.sz = num(attr(rPr, 'sz')) / 100;
  if (attr(rPr, 'b')) o.b = attr(rPr, 'b') === '1' || attr(rPr, 'b') === 'true';
  if (attr(rPr, 'i')) o.i = attr(rPr, 'i') === '1' || attr(rPr, 'i') === 'true';
  if (attr(rPr, 'u') && attr(rPr, 'u') !== 'none') o.u = true;
  if (attr(rPr, 'strike') && attr(rPr, 'strike') !== 'noStrike') o.strike = true;
  const f = parseFill(rPr, ctx);
  if (f && f.type === 'solid') o.color = f.color;
  else if (f && f.type === 'grad' && f.stops.length) o.color = f.stops[0].color;
  const latin = attr(kid(rPr, 'latin'), 'typeface'), ea = attr(kid(rPr, 'ea'), 'typeface');
  if (latin) o.latin = latin;
  if (ea) o.ea = ea;
  if (attr(rPr, 'cap') && attr(rPr, 'cap') !== 'none') o.cap = attr(rPr, 'cap');
  if (attr(rPr, 'baseline')) o.baseline = num(attr(rPr, 'baseline')) / 100000;
  return o;
}
/** 문단 속성 한 겹 (pPr·lvlNpPr) → { algn, lnSpc, spcBef, spcAft, bullet, marL, indent } */
function paraLayer(pPr) {
  if (!pPr) return null;
  const o = {};
  if (attr(pPr, 'algn')) o.algn = attr(pPr, 'algn');
  if (attr(pPr, 'marL')) o.marL = num(attr(pPr, 'marL')) / EMU_PT;
  if (attr(pPr, 'indent')) o.indent = num(attr(pPr, 'indent')) / EMU_PT;
  const sp = n => { const pct = kid(n, 'spcPct'), pts = kid(n, 'spcPts'); return pct ? { pct: num(attr(pct, 'val')) / 100000 } : pts ? { pts: num(attr(pts, 'val')) / 100 } : null; };
  if (kid(pPr, 'lnSpc')) o.lnSpc = sp(kid(pPr, 'lnSpc'));
  if (kid(pPr, 'spcBef')) o.spcBef = sp(kid(pPr, 'spcBef'));
  if (kid(pPr, 'spcAft')) o.spcAft = sp(kid(pPr, 'spcAft'));
  if (kid(pPr, 'buNone')) o.bullet = { type: 'none' };
  else if (kid(pPr, 'buChar')) o.bullet = { type: 'char', char: attr(kid(pPr, 'buChar'), 'char') || '•' };
  else if (kid(pPr, 'buAutoNum')) o.bullet = { type: 'num', scheme: attr(kid(pPr, 'buAutoNum'), 'type') || 'arabicPeriod', startAt: num(attr(kid(pPr, 'buAutoNum'), 'startAt'), 1) };
  else if (kid(pPr, 'buBlip')) o.bullet = { type: 'char', char: '•' };
  if (kid(pPr, 'defRPr')) o.defRPr = kid(pPr, 'defRPr');
  return o;
}
const ROMAN = n => { const t = [[1000, 'M'], [900, 'CM'], [500, 'D'], [400, 'CD'], [100, 'C'], [90, 'XC'], [50, 'L'], [40, 'XL'], [10, 'X'], [9, 'IX'], [5, 'V'], [4, 'IV'], [1, 'I']]; let s = ''; for (const [v, r] of t) while (n >= v) { s += r; n -= v; } return s; };
function numberLabel(scheme, n) {
  const s = scheme || 'arabicPeriod';
  let body = String(n);
  if (/^alphaLc/.test(s)) body = String.fromCharCode(96 + ((n - 1) % 26) + 1);
  else if (/^alphaUc/.test(s)) body = String.fromCharCode(64 + ((n - 1) % 26) + 1);
  else if (/^romanLc/.test(s)) body = ROMAN(n).toLowerCase();
  else if (/^romanUc/.test(s)) body = ROMAN(n);
  else if (/^circleNum/.test(s)) body = n >= 1 && n <= 20 ? String.fromCharCode(0x2460 + n - 1) : String(n);
  else if (/^ea1Chs|^ea1Cht|^ea1Jpn|^hebrew|^arabic1|^arabic2|^hindi|^thai/.test(s)) body = String(n);
  if (/ParenBoth$/.test(s)) return `(${body})`;
  if (/ParenR$/.test(s)) return `${body})`;
  if (/Period$/.test(s)) return `${body}.`;
  return body;
}
/**
 * 글 몸통(txBody) → { bodyPr, paras } — 스타일 사슬 chain = [lstStyle 겹…] (가까운 것부터: 도형 lstStyle · 레이아웃 개체 틀 · 마스터 개체 틀 · 마스터 글 스타일 · 기본 글 스타일)
 * fontRef = 도형 스타일의 글꼴·색 참조 (p:style/a:fontRef)
 */
function parseTextBody(txBody, chain, fontRef, ctx, bodyChain = []) {
  if (!txBody) return null;
  const bp = kid(txBody, 'bodyPr');
  const bodyNodes = [bp, ...bodyChain].filter(Boolean);
  const bpAttr = (name, d) => { for (const b of bodyNodes) if (attr(b, name)) return attr(b, name); return d; };
  const ins = (name, d) => { for (const b of bodyNodes) if (attr(b, name) !== '') return num(attr(b, name)) / EMU_PT; return d; };
  let autofit = null;
  for (const b of bodyNodes) {
    const na = kid(b, 'normAutofit');
    if (na) { autofit = { fontScale: num(attr(na, 'fontScale'), 100000) / 100000, lnSpcRed: num(attr(na, 'lnSpcReduction'), 0) / 100000 }; break; }
    if (kid(b, 'spAutoFit') || kid(b, 'noAutofit')) break;
  }
  const bodyPr = {
    anchor: bpAttr('anchor', 't'), wrap: bpAttr('wrap', 'square') !== 'none', vert: bpAttr('vert', 'horz'),
    lIns: ins('lIns', 7.2), rIns: ins('rIns', 7.2), tIns: ins('tIns', 3.6), bIns: ins('bIns', 3.6), autofit,
    anchorCtr: bpAttr('anchorCtr', '0') === '1',
  };
  const lst = levels(kid(txBody, 'lstStyle'));
  const fontRefCol = fontRef ? resolveColor(colorKid(fontRef), ctx) : null;
  const fontRefIdx = fontRef ? attr(fontRef, 'idx') : '';
  const paras = [];
  const counters = {};
  for (const p of kids(txBody, 'p')) {
    const pPr = kid(p, 'pPr');
    const lvl = num(attr(pPr, 'lvl'));
    // 문단 겹: 이 문단 → 도형 lstStyle 레벨 → 사슬의 레벨 (같은 레벨 없으면 1레벨로 내려가지 않는다 — PowerPoint 도 그렇다)
    const layers = [paraLayer(pPr), paraLayer(lst[lvl]), ...chain.map(L => paraLayer(L[lvl]))].filter(Boolean);
    const pick = k => { for (const L of layers) if (L[k] != null) return L[k]; return undefined; };
    const defLayers = layers.map(L => runLayer(L.defRPr, ctx)).filter(Boolean);
    const runs = [];
    for (const r of p.c) {
      if (r.ln !== 'r' && r.ln !== 'br' && r.ln !== 'fld') continue;
      if (r.ln === 'br') { runs.push({ text: '\n', br: true }); continue; }
      const text = textOf(kid(r, 't'));
      if (!text && r.ln === 'r') continue;
      const own = runLayer(kid(r, 'rPr'), ctx);
      const all = [own, ...defLayers].filter(Boolean);
      const g = k => { for (const L of all) if (L[k] != null) return L[k]; return undefined; };
      let latin = g('latin'), ea = g('ea');
      if (!latin && fontRefIdx) latin = fontRefIdx === 'major' ? '+mj-lt' : '+mn-lt';
      if (!ea && fontRefIdx) ea = fontRefIdx === 'major' ? '+mj-ea' : '+mn-ea';
      let color = g('color');
      if (!color && fontRefCol) color = fontRefCol.hex;
      runs.push({
        text: r.ln === 'fld' && !text ? '' : text, sz: g('sz') || 18, b: !!g('b'), i: !!g('i'), u: !!g('u'), strike: !!g('strike'),
        color: color || '#000000', latin: themeFont(latin || '+mn-lt', ctx.theme) || ctx.theme.fonts.minor.latin || 'Arial', ea: themeFont(ea || '+mn-ea', ctx.theme, true) || themeFont(latin || '+mn-lt', ctx.theme) || '',
        cap: g('cap') || '', baseline: g('baseline') || 0,
      });
    }
    // 빈 문단은 끝 글 속성(endParaRPr)의 크기로 줄 높이를 잡는다
    const endSz = runLayer(kid(p, 'endParaRPr'), ctx);
    let bullet = pick('bullet');
    if (bullet && bullet.type === 'num') { counters[lvl] = (counters[lvl] || bullet.startAt - 1) + 1; bullet = { type: 'char', char: numberLabel(bullet.scheme, counters[lvl]) }; }
    const lnSpc = pick('lnSpc');
    paras.push({
      lvl, algn: pick('algn') || 'l', lnSpc: lnSpc ? { ...lnSpc } : null, spcBef: pick('spcBef') || null, spcAft: pick('spcAft') || null,
      marL: pick('marL') || 0, indent: pick('indent') || 0,
      bullet: bullet && bullet.type === 'char' && runs.some(r => r.text.trim()) ? bullet.char : '',
      runs, endSz: (endSz && endSz.sz) || (defLayers.find(L => L.sz) || {}).sz || 18,
    });
  }
  if (autofit) for (const p of paras) { for (const r of p.runs) if (r.sz) r.sz = r.sz * autofit.fontScale; if (p.lnSpc && p.lnSpc.pct) p.lnSpc.pct = Math.max(0.5, p.lnSpc.pct - autofit.lnSpcRed); }
  return { bodyPr, paras, hasText: paras.some(p => p.runs.some(r => r.text.trim())) };
}

// ---------------- 도형 나무 ----------------
/** 그룹 좌표 변환: 자식 좌표(chOff·chExt 기준) → 부모 좌표 */
function mapRect(r, g) {
  if (!g || !g.ch || !g.ch.w || !g.ch.h) return r;
  const sx = g.w / g.ch.w, sy = g.h / g.ch.h;
  return { ...r, x: g.x + (r.x - g.ch.x) * sx, y: g.y + (r.y - g.ch.y) * sy, w: r.w * sx, h: r.h * sy };
}
function parsePh(nvPr) {
  const ph = kid(nvPr, 'ph');
  return ph ? { type: attr(ph, 'type', ''), idx: attr(ph, 'idx', '') } : null;
}
/** 개체 틀 짝 찾기: (type, idx) 같음 → idx 같음 → type 같음 → 제목/본문끼리 */
function matchPh(ph, list) {
  if (!ph) return null;
  const same = (a, b) => a === b || (!a && b === 'body') || (a === 'body' && !b);
  const t = ph.type || (ph.idx === '0' || !ph.idx ? 'body' : '');
  let m = list.find(s => s.ph && same(s.ph.type || 'body', t || 'body') && s.ph.idx === ph.idx);
  if (!m && ph.idx) m = list.find(s => s.ph && s.ph.idx === ph.idx);
  if (!m && ph.type) m = list.find(s => s.ph && s.ph.type === ph.type);
  if (!m) { const kind = phStyleKind(ph); m = list.find(s => s.ph && phStyleKind(s.ph) === kind && kind !== 'other'); }
  return m || null;
}
/**
 * spTree 한 그루 → 도형 목록 (그룹은 풀어 좌표를 바꾸고, 돌린 그룹은 gt 에 변환을 남긴다)
 * origin: 'slide' | 'layout' | 'master' | 'dgm'
 */
function walkTree(tree, ctx, origin, group = null, gts = [], depth = 0) {
  const out = [];
  if (!tree || depth > 12) return out;
  for (const c of tree.c) {
    if (c.ln === 'AlternateContent') { // 새 기능(p14 등)은 Fallback 으로
      const fb = kid(c, 'Fallback') || kid(c, 'Choice');
      out.push(...walkTree(fb, ctx, origin, group, gts, depth + 1));
      continue;
    }
    if (!['sp', 'pic', 'cxnSp', 'graphicFrame', 'grpSp', 'contentPart'].includes(c.ln)) continue;
    const nv = c.c.find(x => /^nv.*Pr$/.test(x.ln));
    const cNvPr = kid(nv, 'cNvPr');
    const nvPr = kid(nv, 'nvPr');
    if (attr(cNvPr, 'hidden') === '1') continue;
    const id = attr(cNvPr, 'id'), name = attr(cNvPr, 'name');
    if (c.ln === 'grpSp') {
      const gx = parseXfrm(kid(c, 'grpSpPr'));
      if (!gx) continue;
      const mine = group ? mapRect(gx, group) : gx;
      // 그룹 자체의 회전·뒤집기는 자식에게 변환으로 전달 (자식 좌표는 그룹 안 좌표계로 계속 바꾼다)
      const g2 = { ...mine, ch: gx.ch };
      const gts2 = gx.rot || gx.flipH || gx.flipV ? [...gts, { cx: mine.x + mine.w / 2, cy: mine.y + mine.h / 2, rot: gx.rot, flipH: gx.flipH, flipV: gx.flipV }] : gts;
      out.push(...walkTree(c, ctx, origin, g2, gts2, depth + 1));
      continue;
    }
    const spPr = kid(c, 'spPr');
    const styleNode = kid(c, 'style');
    let xf = parseXfrm(c.ln === 'graphicFrame' ? c : spPr);
    const ph = parsePh(nvPr);
    const sh = { id, name, origin, kind: c.ln, ph, xf, gt: gts, spPr, styleNode, txBody: kid(c, 'txBody'), node: c, group: !!group };
    if (xf && group) sh.xf = { ...mapRect(xf, group), rot: xf.rot, flipH: xf.flipH, flipV: xf.flipV, present: xf.present };
    if (c.ln === 'pic') {
      const bf = kid(c, 'blipFill');
      sh.pic = parseFill({ c: [bf] }, ctx);
      const media = kid(nvPr, 'videoFile') || kid(nvPr, 'audioFile');
      if (media) sh.media = media.ln;
    }
    if (c.ln === 'graphicFrame') {
      const gd = kid(kid(c, 'graphic'), 'graphicData');
      const uri = attr(gd, 'uri');
      if (/\/table$/.test(uri)) { sh.kind = 'table'; sh.tbl = kid(gd, 'tbl'); }
      else if (/\/chart$/.test(uri)) sh.kind = 'chart';
      else if (/\/diagram$/.test(uri)) { sh.kind = 'diagram'; sh.dgm = kid(gd, 'relIds'); }
      else if (/oleObject|\/ole$/.test(uri)) { sh.kind = 'ole'; const p = find(gd, 'pic'); if (p) { sh.pic = parseFill({ c: [kid(p, 'blipFill')] }, ctx); } }
      else sh.kind = 'graphic';
    }
    out.push(sh);
  }
  return out;
}

/** 배경 (p:bg) → 채우기 (없으면 null) */
function parseBg(cSld, ctx) {
  const bg = kid(cSld, 'bg');
  if (!bg) return null;
  const pr = kid(bg, 'bgPr');
  if (pr) return parseFill(pr, ctx);
  const ref = kid(bg, 'bgRef');
  if (ref) return themeRefFill(ref, ctx);
  return null;
}

// ---------------- 프레젠테이션 ----------------
/** 파일 열기 → { w, h (pt), slides: [{ part, n }], pkg, theme… } — 숨긴 슬라이드는 뺀다 */
function openPptx(file) {
  const buf = Buffer.isBuffer(file) ? file : fs.readFileSync(file);
  const pkg = new Pkg(buf);
  const presPart = pkg.relByType('', 'officeDocument') || 'ppt/presentation.xml';
  const pres = pkg.xml(presPart);
  if (!pres) throw new Error('pptx 파일 형식이 아닙니다 (presentation.xml 없음).');
  const sz = kid(pres, 'sldSz');
  const w = num(attr(sz, 'cx'), 9144000) / EMU_PT, h = num(attr(sz, 'cy'), 6858000) / EMU_PT;
  const defaultText = levels(kid(pres, 'defaultTextStyle'));
  const slides = [];
  for (const s of kids(kid(pres, 'sldIdLst'), 'sldId')) {
    const part = pkg.relTarget(presPart, attr(s, 'r:id') || attr(s, 'id'));
    if (!part || !pkg.has(part)) continue;
    const x = pkg.xml(part);
    if (!x || attr(x, 'show') === '0') continue; // 숨긴 슬라이드
    slides.push({ part });
  }
  return { pkg, presPart, w, h, defaultText, slides };
}

/** 마스터·레이아웃 부분 읽기 (캐시) */
function loadMaster(doc, part) {
  if (!doc._masters) doc._masters = new Map();
  if (doc._masters.has(part)) return doc._masters.get(part);
  const pkg = doc.pkg;
  const x = pkg.xml(part);
  const themePart = pkg.relByType(part, 'theme');
  const theme = themePart && pkg.has(themePart) ? parseTheme(pkg.xml(themePart)) : { colors: {}, fonts: { major: { latin: '', ea: '' }, minor: { latin: '', ea: '' } }, fills: [], bgFills: [], lines: [] };
  const clrMap = parseClrMap(kid(x, 'clrMap'));
  const ctx = { pkg, part, theme, clrMap };
  const cSld = kid(x, 'cSld');
  const tx = kid(x, 'txStyles');
  const m = {
    part, theme, clrMap, ctx, bg: parseBg(cSld, ctx), shapes: walkTree(kid(cSld, 'spTree'), ctx, 'master'),
    styles: { title: levels(kid(tx, 'titleStyle')), body: levels(kid(tx, 'bodyStyle')), other: levels(kid(tx, 'otherStyle')) },
  };
  doc._masters.set(part, m);
  return m;
}
function loadLayout(doc, part) {
  if (!doc._layouts) doc._layouts = new Map();
  if (doc._layouts.has(part)) return doc._layouts.get(part);
  const pkg = doc.pkg;
  const x = pkg.xml(part);
  const masterPart = pkg.relByType(part, 'slideMaster');
  const master = masterPart && pkg.has(masterPart) ? loadMaster(doc, masterPart) : null;
  const ovr = kid(kid(x, 'clrMapOvr'), 'overrideClrMapping');
  const clrMap = ovr ? parseClrMap(ovr) : master ? master.clrMap : {};
  const ctx = { pkg, part, theme: master ? master.theme : parseTheme({ c: [] }), clrMap };
  const cSld = kid(x, 'cSld');
  const L = { part, master, ctx, clrMap, bg: parseBg(cSld, ctx), shapes: walkTree(kid(cSld, 'spTree'), ctx, 'layout'), showMasterSp: attr(x, 'showMasterSp', '1') !== '0' };
  doc._layouts.set(part, L);
  return L;
}

/**
 * 슬라이드 하나 → { w, h, bg, shapes: [도형…] } — 도형은 쌓인 차례(뒤 → 앞): 마스터 장식 · 레이아웃 장식 · 슬라이드 도형
 *   도형 = { id, name, origin, kind: sp|pic|cxnSp|table|chart|diagram|ole|graphic, ph, x, y, w, h (pt), rot, flipH, flipV, gt,
 *            geom: { prst, adj, custom }, fill, line, text: { bodyPr, paras }, pic: { part, crop, alpha }, table, dgmShapes }
 */
function parseSlide(doc, k) {
  const { pkg } = doc;
  const s = doc.slides[k];
  const x = pkg.xml(s.part);
  const layoutPart = pkg.relByType(s.part, 'slideLayout');
  const layout = layoutPart && pkg.has(layoutPart) ? loadLayout(doc, layoutPart) : null;
  const master = layout ? layout.master : null;
  const ovr = kid(kid(x, 'clrMapOvr'), 'overrideClrMapping');
  const clrMap = ovr ? parseClrMap(ovr) : layout ? layout.clrMap : {};
  const theme = master ? master.theme : parseTheme({ c: [] });
  const ctx = { pkg, part: s.part, theme, clrMap };
  const cSld = kid(x, 'cSld');
  const own = walkTree(kid(cSld, 'spTree'), ctx, 'slide');
  const showMaster = attr(x, 'showMasterSp', '1') !== '0' && (!layout || layout.showMasterSp);
  const bg = parseBg(cSld, ctx) || (layout && layout.bg) || (master && master.bg) || { type: 'solid', color: '#ffffff', alpha: 1 };
  // 장식: 레이아웃의 개체 틀이 아닌 도형 · 마스터의 개체 틀이 아닌 도형 (레이아웃·마스터의 개체 틀은 슬라이드에 그리지 않는다)
  const deco = [];
  if (master && showMaster) for (const sh of master.shapes) if (!sh.ph) deco.push(finishShape(sh, master.ctx, doc, [], [master, layout]));
  if (layout) for (const sh of layout.shapes) if (!sh.ph) deco.push(finishShape(sh, layout.ctx, doc, [], [master, layout]));
  const shapes = [];
  for (const sh of own) {
    // 개체 틀: 레이아웃 → 마스터 짝에서 위치·글 스타일을 물려받는다
    let chain = [], bodyChain = [], fontRef = kid(sh.styleNode, 'fontRef');
    let lay = null, mas = null;
    if (sh.ph) {
      lay = layout ? matchPh(sh.ph, layout.shapes) : null;
      mas = master ? matchPh(lay && lay.ph ? lay.ph : sh.ph, master.shapes) : null;
      if (!sh.xf || !sh.xf.present) { const src = (lay && lay.xf && lay.xf.present && lay) || (mas && mas.xf && mas.xf.present && mas); if (src) sh.xf = { ...src.xf }; }
      if (lay && lay.txBody) { chain.push(levels(kid(lay.txBody, 'lstStyle'))); bodyChain.push(kid(lay.txBody, 'bodyPr')); }
      if (mas && mas.txBody) { chain.push(levels(kid(mas.txBody, 'lstStyle'))); bodyChain.push(kid(mas.txBody, 'bodyPr')); }
      if (!kid(sh.spPr, 'solidFill') && !kid(sh.spPr, 'noFill') && !kid(sh.spPr, 'gradFill') && !kid(sh.spPr, 'blipFill')) sh.inheritFill = (lay && lay.spPr) || (mas && mas.spPr) || null;
      if (!fontRef) fontRef = kid((lay || {}).styleNode, 'fontRef') || kid((mas || {}).styleNode, 'fontRef');
    }
    const kind = phStyleKind(sh.ph);
    if (master) chain.push(master.styles[kind] || []);
    chain.push(doc.defaultText);
    shapes.push(finishShape(sh, ctx, doc, chain, [master, layout], bodyChain, fontRef));
  }
  return { w: doc.w, h: doc.h, bg, shapes: [...deco, ...shapes].filter(Boolean) };
}
/** 도형의 나머지(모양·채우기·선·글·그림·표)를 채운다 → 좌표는 pt */
function finishShape(sh, ctx, doc, chain, parents, bodyChain = [], fontRef = null) {
  if (!sh.xf) return null;
  const pt = v => v / EMU_PT;
  const o = {
    id: sh.id, name: sh.name, origin: sh.origin, kind: sh.kind, ph: sh.ph,
    x: pt(sh.xf.x), y: pt(sh.xf.y), w: pt(sh.xf.w), h: pt(sh.xf.h), rot: sh.xf.rot || 0, flipH: !!sh.xf.flipH, flipV: !!sh.xf.flipV,
    gt: sh.gt.map(g => ({ cx: pt(g.cx), cy: pt(g.cy), rot: g.rot, flipH: g.flipH, flipV: g.flipV })),
    geom: null, fill: null, line: null, text: null, pic: null, table: null, dgmShapes: null, inGroup: sh.group,
  };
  const spPr = sh.spPr;
  if (spPr) {
    const prst = kid(spPr, 'prstGeom');
    const cust = kid(spPr, 'custGeom');
    if (prst) o.geom = { prst: attr(prst, 'prst'), adj: Object.fromEntries(kids(kid(prst, 'avLst'), 'gd').map(g => [attr(g, 'name'), num(String(attr(g, 'fmla')).replace(/^val\s+/, ''))])) };
    else if (cust) o.geom = { prst: 'custom', adj: {}, custom: parseCustGeom(cust) };
    else if (sh.kind === 'sp' || sh.kind === 'cxnSp') o.geom = { prst: 'rect', adj: {} };
    let fill = parseFill(spPr, ctx);
    if (!fill && sh.inheritFill) fill = parseFill(sh.inheritFill, ctx);
    if (!fill && kid(sh.styleNode, 'fillRef') && sh.kind !== 'pic') fill = themeRefFill(kid(sh.styleNode, 'fillRef'), ctx);
    if (fill && fill.type === 'group') fill = null;
    o.fill = fill && fill.type !== 'none' ? fill : null;
    o.line = parseLine(spPr, sh.styleNode, ctx);
    if (sh.kind === 'cxnSp' && !o.line) o.line = { w: 0.75, color: '#000000', alpha: 1, dash: '', head: 'none', tail: 'none' };
  }
  if (sh.pic && sh.pic.type === 'blip') o.pic = sh.pic;
  if (sh.media) o.media = sh.media;
  if (sh.txBody && sh.kind !== 'table') o.text = parseTextBody(sh.txBody, chain, fontRef, ctx, bodyChain);
  if (sh.kind === 'table' && sh.tbl) o.table = parseTable(sh.tbl, ctx, doc, o);
  if (sh.kind === 'diagram' && sh.dgm) o.dgmShapes = parseDiagram(sh, ctx, doc, o);
  return o;
}

/** 사용자 지정 모양(custGeom) → { w, h, paths: [{ d(가로 w·세로 h 기준), fill, stroke }] } */
function parseCustGeom(cust) {
  const out = [];
  for (const p of kids(kid(cust, 'pathLst'), 'path')) {
    const w = num(attr(p, 'w')), h = num(attr(p, 'h'));
    const cmds = [];
    let cx = 0, cy = 0;
    const ptOf = n => { const t = kid(n, 'pt'); return [num(attr(t, 'x')), num(attr(t, 'y'))]; };
    for (const c of p.c) {
      if (c.ln === 'moveTo') { [cx, cy] = ptOf(c); cmds.push(['M', cx, cy]); }
      else if (c.ln === 'lnTo') { [cx, cy] = ptOf(c); cmds.push(['L', cx, cy]); }
      else if (c.ln === 'cubicBezTo') { const pts = kids(c, 'pt').map(t => [num(attr(t, 'x')), num(attr(t, 'y'))]); if (pts.length === 3) { cmds.push(['C', ...pts.flat()]); [cx, cy] = pts[2]; } }
      else if (c.ln === 'quadBezTo') { const pts = kids(c, 'pt').map(t => [num(attr(t, 'x')), num(attr(t, 'y'))]); if (pts.length === 2) { cmds.push(['Q', ...pts.flat()]); [cx, cy] = pts[1]; } }
      else if (c.ln === 'arcTo') {
        const wR = num(attr(c, 'wR')), hR = num(attr(c, 'hR')), st = num(attr(c, 'stAng')) / 60000, sw = num(attr(c, 'swAng')) / 60000;
        const rad = a => a * Math.PI / 180;
        const ox = cx - wR * Math.cos(rad(st)), oy = cy - hR * Math.sin(rad(st));
        const ex = ox + wR * Math.cos(rad(st + sw)), ey = oy + hR * Math.sin(rad(st + sw));
        cmds.push(['A', wR, hR, 0, Math.abs(sw) > 180 ? 1 : 0, sw > 0 ? 1 : 0, ex, ey]);
        cx = ex; cy = ey;
      }
      else if (c.ln === 'close') cmds.push(['Z']);
    }
    out.push({ w, h, cmds, fill: attr(p, 'fill', 'norm') !== 'none', stroke: attr(p, 'stroke', '1') !== '0' });
  }
  return out;
}

/** 표 → { cols: [pt], rows: [{ h, cells: [{ text, fill, span, rowSpan, merged }] }], firstRow, bandRow, accent } */
function parseTable(tbl, ctx, doc, frame) {
  const tblPr = kid(tbl, 'tblPr');
  const cols = kids(kid(tbl, 'tblGrid'), 'gridCol').map(c => num(attr(c, 'w')) / EMU_PT);
  const styleId = textOf(kid(tblPr, 'tableStyleId'));
  const STYLE_ACCENT = { '{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}': 'accent1', '{21E4AEA4-8DFA-4A89-87EB-49C32662AFE0}': 'accent2', '{F5AB1C69-6EDB-4FF4-983F-18BD219EF322}': 'accent3', '{00A15C55-8517-42AA-B614-E9B94910E393}': 'accent4', '{7DF18680-E054-41AD-8BC1-D1AEF772440D}': 'accent5', '{93296810-A885-4BE3-A3E7-6D5BEEA58F35}': 'accent6', '{3C2FFA5D-87B4-456A-9821-1D502468CF0F}': 'accent1' };
  const accentKey = STYLE_ACCENT[styleId] || (styleId ? 'accent1' : '');
  const accent = accentKey && ctx.theme.colors[accentKey] ? '#' + ctx.theme.colors[accentKey] : '';
  const rows = kids(tbl, 'tr').map(tr => ({
    h: num(attr(tr, 'h')) / EMU_PT,
    cells: kids(tr, 'tc').map(tc => {
      const tcPr = kid(tc, 'tcPr');
      const fill = parseFill(tcPr, ctx);
      const text = parseTextBody(kid(tc, 'txBody'), [doc.defaultText], null, ctx);
      const borders = {};
      for (const side of ['L', 'R', 'T', 'B']) { const ln = kid(tcPr, 'ln' + side); if (ln) { const f = parseFill(ln, ctx); borders[side] = f && f.type === 'solid' ? { w: num(attr(ln, 'w'), 12700) / EMU_PT, color: f.color } : f && f.type === 'none' ? null : undefined; } }
      return { text, fill: fill && fill.type !== 'none' ? fill : null, span: num(attr(tc, 'gridSpan'), 1), rowSpan: num(attr(tc, 'rowSpan'), 1), merged: attr(tc, 'hMerge') === '1' || attr(tc, 'vMerge') === '1', borders, ins: { l: num(attr(tcPr, 'marL'), 91440) / EMU_PT, r: num(attr(tcPr, 'marR'), 91440) / EMU_PT, t: num(attr(tcPr, 'marT'), 45720) / EMU_PT, b: num(attr(tcPr, 'marB'), 45720) / EMU_PT }, anchor: attr(tcPr, 'anchor', 't') };
    }),
  }));
  return { cols, rows, firstRow: attr(tblPr, 'firstRow') === '1', bandRow: attr(tblPr, 'bandRow') === '1', accent, styled: !!styleId };
}

/** SmartArt: 그려 둔 도형(diagrams/drawingN.xml 의 dsp:sp) → 도형 목록 (좌표는 틀 기준 → 슬라이드 기준으로) */
function parseDiagram(sh, ctx, doc, frame) {
  const part = doc.pkg.relByType(ctx.part, 'diagramDrawing');
  if (!part || !doc.pkg.has(part)) return null;
  const x = doc.pkg.xml(part);
  const tree = kid(x, 'spTree');
  if (!tree) return null;
  const dctx = { ...ctx, part };
  const raw = walkTree(tree, dctx, 'dgm');
  const out = [];
  for (const d of raw) {
    const f = finishShape(d, dctx, doc, [doc.defaultText], [], [], kid(d.styleNode, 'fontRef'));
    if (!f) continue;
    f.x += frame.x; f.y += frame.y;
    f.gt = f.gt.map(g => ({ ...g, cx: g.cx + frame.x, cy: g.cy + frame.y }));
    f.origin = 'slide'; f.fromDiagram = true;
    out.push(f);
  }
  return out;
}

/** 슬라이드의 글 (위에서 아래 · 왼쪽에서 오른쪽, 쪽 번호·바닥글·날짜 자리는 뺀다) — 자막·나누기 AI 에 쓴다 */
function slideText(slide) {
  const boxes = [];
  for (const sh of slide.shapes) {
    if (sh.origin !== 'slide') continue;
    if (sh.ph && ['sldNum', 'ftr', 'dt'].includes(sh.ph.type)) continue;
    const list = sh.dgmShapes || [sh];
    for (const s of list) {
      if (s.text && s.text.hasText) boxes.push({ t: s.y, l: s.x, text: textLines(s.text).join('\n') });
      if (s.table) boxes.push({ t: s.y, l: s.x, text: s.table.rows.map(r => r.cells.filter(c => !c.merged && c.text).map(c => textLines(c.text).join(' ')).join(' | ')).join('\n') });
    }
  }
  return boxes.sort((a, b) => a.t - b.t || a.l - b.l).map(b => b.text).filter(t => t.trim()).join('\n');
}
/** 글 몸통 → 줄 목록 (문단마다, 글머리표 붙여서) */
function textLines(text) {
  return text.paras.map(p => (p.bullet ? p.bullet + ' ' : '') + p.runs.map(r => r.text).join('').replace(/\n$/, ''));
}

module.exports = { parseXml, kid, kids, find, attr, textOf, Pkg, openPptx, parseSlide, slideText, textLines, resolveColor, parseFill, parseTheme, themeFont, numberLabel, EMU_PT };
