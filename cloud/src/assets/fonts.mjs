// 글꼴 파일 안의 이름 · 굵기 읽기 (TTF · OTF · TTC 첫 글꼴 · WOFF). WOFF2 는 압축 방식이 달라 읽지 않는다 (이름 · 굵기를 적어 받는다).
// 왜: '구세군체 Bold' 처럼 이름은 Bold 인데 파일 안 굵기 값(OS/2 usWeightClass)이 400 인 글꼴은, 브라우저가 파일 값을 믿으면
// Regular 와 같은 글꼴로 보고 하나를 숨긴다 (render.js localFontAlias 설명, 2026-11-10). 클라우드에서는 @font-face 에
// 이름 · 굵기를 사람이 적은 값으로 따로 쓰므로 그 문제가 없다 — 여기서는 처음 값을 제안하고, 이름과 굵기 값이 어긋나면 알린다.
// Cloudflare 전용 API 를 쓰지 않는다 (DecompressionStream 은 Workers · Node 둘 다 있음).

// 영어 낱말은 앞이 영문자가 아닐 때만 ('Something' 의 thin · 'Notebook' 의 book 을 굵기로 읽지 않게)
const W = (src, w) => [new RegExp(`(?<![a-z])(?:${src})`, 'i'), w];
const WEIGHT_WORDS = [
  W('(?:extra|ultra)[\\s-]*bold', 800), W('(?:semi|demi)[\\s-]*bold', 600), W('(?:extra|ultra)[\\s-]*light', 200),
  W('hairline|thin', 100), W('black|heavy', 900), W('bold', 700), W('medium', 500), W('light', 300), W('regular|normal|book|roman', 400),
  [/굵게|볼드/, 700], [/가늘게|라이트/, 300],
];
/** 'Bold' · 'SemiBold Italic' · '구세군체 Bold' → 700 · 600 · 700. 굵기 낱말이 없으면 0 */
export function weightFromName(s) {
  for (const [re, w] of WEIGHT_WORDS) if (re.test(String(s || ''))) return w;
  return 0;
}

function utf16be(u, off, len) {
  let s = '';
  for (let i = 0; i + 1 < len; i += 2) s += String.fromCharCode((u[off + i] << 8) | u[off + i + 1]);
  return s;
}
function latin1(u, off, len) {
  let s = '';
  for (let i = 0; i < len; i++) s += String.fromCharCode(u[off + i]);
  return s;
}

/** sfnt 표 목록: { tag → Uint8Array(그 표) }. woff 면 압축을 푼다 */
async function tables(u) {
  const dv = new DataView(u.buffer, u.byteOffset, u.byteLength);
  const tag = o => String.fromCharCode(u[o], u[o + 1], u[o + 2], u[o + 3]);
  const sig = tag(0);
  const out = {};
  const want = new Set(['name', 'OS/2', 'head']);
  if (sig === 'wOFF') {
    const n = dv.getUint16(12);
    for (let i = 0; i < n; i++) {
      const o = 44 + i * 20;
      const t = tag(o), off = dv.getUint32(o + 4), comp = dv.getUint32(o + 8), orig = dv.getUint32(o + 12);
      if (!want.has(t) || off + comp > u.length) continue;
      const slice = u.subarray(off, off + comp);
      if (comp === orig) out[t] = slice;
      else {
        const ds = new Blob([slice]).stream().pipeThrough(new DecompressionStream('deflate'));
        out[t] = new Uint8Array(await new Response(ds).arrayBuffer());
      }
    }
    return { format: 'woff', out };
  }
  let base = 0, format = sig === 'OTTO' ? 'otf' : 'ttf';
  if (sig === 'ttcf') { base = dv.getUint32(12); format = 'ttc'; }
  else if (sig === 'wOF2') return { format: 'woff2', out };
  const v = dv.getUint32(base);
  if (![0x00010000, 0x4f54544f, 0x74727565, 0x74797031].includes(v)) return { format: 'unknown', out };
  const n = dv.getUint16(base + 4);
  for (let i = 0; i < n; i++) {
    const o = base + 12 + i * 16;
    if (o + 16 > u.length) break;
    const t = tag(o), off = dv.getUint32(o + 8), len = dv.getUint32(o + 12);
    if (want.has(t) && off + len <= u.length) out[t] = u.subarray(off, off + len);
  }
  return { format, out };
}

/** name 표 → { id → [{platform, lang, text}] } */
function names(t) {
  const res = {};
  if (!t || t.length < 6) return res;
  const dv = new DataView(t.buffer, t.byteOffset, t.byteLength);
  const count = dv.getUint16(2), strOff = dv.getUint16(4);
  for (let i = 0; i < count; i++) {
    const o = 6 + i * 12;
    if (o + 12 > t.length) break;
    const platform = dv.getUint16(o), enc = dv.getUint16(o + 2), lang = dv.getUint16(o + 4), id = dv.getUint16(o + 6), len = dv.getUint16(o + 8), off = dv.getUint16(o + 10);
    if (![1, 2, 4, 16, 17].includes(id) || strOff + off + len > t.length) continue;
    const text = (platform === 3 || platform === 0) ? utf16be(t, strOff + off, len) : platform === 1 && enc === 0 ? latin1(t, strOff + off, len) : '';
    if (text.trim()) (res[id] ||= []).push({ platform, lang, text: text.replace(/\u0000/g, '').trim() });
  }
  return res;
}
/** 한국어(0x0412) → 영어(0x0409) → 윈도우 아무것 → 그 밖 */
function pick(list, lang) {
  if (!list?.length) return '';
  const win = list.filter(x => x.platform === 3);
  return (lang && win.find(x => x.lang === lang)?.text) || win.find(x => x.lang === 0x0412)?.text || win.find(x => x.lang === 0x0409)?.text || win[0]?.text || list[0].text;
}

/**
 * 글꼴 파일 → { format, family, familyEn, typoFamily, subfamily, fullName, weightClass, italic,
 *               suggested:{family, weight, style}, warnings:[] }. 읽지 못하면 { format, suggested:{…기본}, warnings }
 * family 제안은 이름 1번(GDI 가족 이름) — 설치판 템플릿에 적히는 이름('구세군체 Bold')과 같은 쪽이다.
 */
export async function fontInfo(bytes) {
  const u = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const warnings = [];
  let format = 'unknown', out = {};
  try { ({ format, out } = await tables(u)); } catch { format = 'unknown'; }
  const n = names(out.name);
  const info = {
    format,
    family: pick(n[1]), familyEn: pick(n[1], 0x0409), typoFamily: pick(n[16]), subfamily: pick(n[17]) || pick(n[2]), fullName: pick(n[4]),
    weightClass: 0, italic: false,
  };
  if (out['OS/2'] && out['OS/2'].length >= 64) {
    const dv = new DataView(out['OS/2'].buffer, out['OS/2'].byteOffset, out['OS/2'].byteLength);
    info.weightClass = dv.getUint16(4);
    info.italic = !!(dv.getUint16(62) & 1);
  } else if (out.head && out.head.length >= 46) {
    info.italic = !!(new DataView(out.head.buffer, out.head.byteOffset).getUint16(44) & 2);
  }
  if (/italic|oblique/i.test(info.subfamily)) info.italic = true;
  const nameWeight = weightFromName(info.subfamily) || weightFromName(info.family) || weightFromName(info.fullName);
  let weight = info.weightClass || nameWeight || 400;
  if (nameWeight && info.weightClass && Math.abs(nameWeight - info.weightClass) >= 200) {
    warnings.push(`파일 안의 굵기 값은 ${info.weightClass} 인데 이름('${info.subfamily || info.family}')은 ${nameWeight} 입니다. 브라우저가 다른 굵기와 겹친다고 보고 숨기지 않게 ${nameWeight} 로 적었습니다 — 다르면 고쳐 주세요.`);
    weight = nameWeight;
  }
  if (format === 'woff2' || format === 'unknown') warnings.push(format === 'woff2' ? 'WOFF2 파일 안의 이름은 읽지 않습니다. 글꼴 이름과 굵기를 적어 주세요.' : '글꼴 파일 안을 읽지 못했습니다. 글꼴 이름과 굵기를 적어 주세요.');
  info.suggested = { family: info.family || info.typoFamily || '', weight, style: info.italic ? 'italic' : 'normal' };
  info.warnings = warnings;
  return info;
}
