'use strict';
// PPT 글 배치용 글꼴 (PPT 요소 따로 고치기 · PowerPoint 없이 슬라이드 그리기)
//   이음오피스 엔진(vendor/ieum-pptx.js)은 글꼴 메트릭(글자 너비 · 올림 · 내림)으로 PowerPoint 처럼 줄을 바꾼다.
//   여기서는 이 PC 에 설치된 글꼴 파일을 찾아(윈도우 C:\Windows\Fonts · %LOCALAPPDATA%\Microsoft\Windows\Fonts,
//   맥 /Library/Fonts · /System/Library/Fonts · ~/Library/Fonts, 리눅스 /usr/share/fonts · ~/.fonts) opentype.js 로 읽고
//   엔진의 OpentypeMetrics 에 준다. 없는 글꼴은 PowerPoint 가 하듯 같은 글자 종류의 글꼴로 바꿔 재고(맑은 고딕 → 설치된 한글 글꼴),
//   그것도 없으면 엔진의 HeuristicMetrics(글자 종류별 비율 추정)로.
//   재는 데 쓴 글꼴 이름(displayFamily)은 SVG 의 font-family 로도 쓴다 — 재기와 그리기가 같은 글꼴이어야 글자가 겹치거나 벌어지지 않는다.
// 글꼴 파일은 한 번 읽어 두고 다시 쓴다 (프로세스가 사는 동안, 24개까지).
const fs = require('fs');
const E = require('./vendor/ieum-pptx');

const HANGUL_RE = /[\u1100-\u11ff\u3130-\u318f\uac00-\ud7a3]/;
const KANA_RE = /[\u3040-\u30ff\uff66-\uff9f]/;
const HAN_RE = /[\u2e80-\u9fff\uf900-\ufaff]/;
const SERIF_RE = /serif|roman|garamond|georgia|cambria|palatino|batang|바탕|myeongjo|명조|gungsuh|궁서|mincho|明朝|song|宋/i;
const MONO_RE = /mono|courier|consolas|menlo|monaco|code|typewriter|gulimche|dotumche|굴림체|돋움체|바탕체/i;
/** 영어 이름이지만 한글 글꼴인 것 (PowerPoint 는 글꼴 이름이 아니라 글자·언어로 대체 글꼴을 고르지만, 이름으로 미리 알 수 있는 것) */
const KO_NAME_RE = /malgun|gulim|dotum|batang|gungsuh|nanum|hy[a-z]|yoon|sandoll|함초롬|hcr |headline|kopub|noto (sans|serif) (cjk )?kr|spoqa|pretendard|gmarket|^(a|un)?(dotum|batang)/i;
const JA_NAME_RE = /meiryo|yu gothic|yu mincho|ms (p)?gothic|ms (p)?mincho|hiragino|noto (sans|serif) (cjk )?jp|游|ゴシック|明朝|メイリオ/i;

const IS_WIN = process.platform === 'win32', IS_MAC = process.platform === 'darwin';
/** 다른 이름으로 설치돼 있는 같은 글꼴 · 치수가 같은 대체 글꼴 (앞부터 찾는다). Calibri↔Carlito · Cambria↔Caladea 는 치수가 같아 '바꿈'으로 치지 않는다 */
const ALIASES = {
  '맑은 고딕': ['Malgun Gothic'], 'malgun gothic': ['맑은 고딕'],
  '맑은 고딕 semilight': ['Malgun Gothic Semilight', 'Malgun Gothic'],
  '바탕': ['Batang'], '바탕체': ['BatangChe', 'Batang'], '궁서': ['Gungsuh'], '궁서체': ['GungsuhChe', 'Gungsuh'],
  '굴림': ['Gulim'], '굴림체': ['GulimChe', 'Gulim'], '돋움': ['Dotum'], '돋움체': ['DotumChe', 'Dotum'],
  '나눔고딕': ['NanumGothic'], '나눔바른고딕': ['NanumBarunGothic', 'NanumGothic'], '나눔명조': ['NanumMyeongjo'], '나눔스퀘어': ['NanumSquare', 'NanumGothic'],
  'nanumgothic': ['나눔고딕'], 'nanummyeongjo': ['나눔명조'],
  calibri: ['Carlito'], 'calibri light': ['Carlito'], cambria: ['Caladea'],
  helvetica: ['Arial'], 'helvetica neue': ['Arial'], arial: ['Liberation Sans', 'Arimo'], 'times new roman': ['Liberation Serif', 'Tinos'],
  'courier new': ['Liberation Mono', 'Cousine'],
  '微软雅黑': ['Microsoft YaHei'], '宋体': ['SimSun'], 'メイリオ': ['Meiryo'], '游ゴシック': ['Yu Gothic'],
};
/** 치수가 같은 짝 (이것으로 바꾼 것은 PowerPoint 도 그대로 커닝한다) */
const METRIC_TWINS = new Set(['calibri>carlito', 'cambria>caladea', 'arial>liberation sans', 'arial>arimo', 'times new roman>liberation serif', 'times new roman>tinos', 'courier new>liberation mono', 'courier new>cousine', '맑은 고딕>malgun gothic', 'malgun gothic>맑은 고딕', '나눔고딕>nanumgothic', 'nanumgothic>나눔고딕']);

/** 없는 글꼴 대신 쓸 글꼴 (글자 종류 → 이 PC 에 있을 법한 것부터) */
const SUBST = {
  ko: IS_WIN ? ['Malgun Gothic', '맑은 고딕', 'Gulim', 'Dotum', 'NanumGothic'] : IS_MAC ? ['Apple SD Gothic Neo', 'AppleGothic', 'Malgun Gothic', 'NanumGothic'] : ['Malgun Gothic', 'NanumGothic', 'NanumBarunGothic', 'Noto Sans CJK KR', 'Noto Sans KR', 'UnDotum', 'Baekmuk Dotum', 'WenQuanYi Zen Hei', 'Droid Sans Fallback', 'Unifont'],
  koSerif: IS_WIN ? ['Batang', '바탕', 'Gungsuh', 'NanumMyeongjo'] : IS_MAC ? ['AppleMyungjo', 'Batang', 'NanumMyeongjo'] : ['Batang', 'NanumMyeongjo', 'Noto Serif CJK KR', 'Noto Serif KR', 'UnBatang', 'Baekmuk Batang'],
  ja: IS_WIN ? ['Yu Gothic', 'Meiryo', 'MS Gothic'] : IS_MAC ? ['Hiragino Sans', 'Hiragino Kaku Gothic ProN'] : ['Noto Sans CJK JP', 'IPAGothic', 'IPAPGothic', 'VL Gothic', 'WenQuanYi Zen Hei'],
  jaSerif: IS_WIN ? ['Yu Mincho', 'MS Mincho'] : IS_MAC ? ['Hiragino Mincho ProN'] : ['Noto Serif CJK JP', 'IPAMincho'],
  zh: IS_WIN ? ['Microsoft YaHei', 'SimSun', 'Microsoft JhengHei'] : IS_MAC ? ['PingFang SC', 'Heiti SC', 'PingFang TC'] : ['Noto Sans CJK SC', 'WenQuanYi Zen Hei', 'WenQuanYi Micro Hei', 'Droid Sans Fallback'],
  sans: ['Calibri', 'Carlito', 'Arial', 'Liberation Sans', 'Arimo', 'Helvetica', 'DejaVu Sans', 'FreeSans'],
  serif: ['Cambria', 'Caladea', 'Times New Roman', 'Liberation Serif', 'Tinos', 'Georgia', 'DejaVu Serif', 'FreeSerif'],
  mono: ['Consolas', 'Courier New', 'Liberation Mono', 'Cousine', 'DejaVu Sans Mono', 'FreeMono'],
};

const key = s => E.fontNameKey(String(s || ''));
/** 'Apercu Light' · '나눔스퀘어 Bold' 같은 '글꼴 굵기' 이름 → 글꼴 이름 */
const WEIGHT_SUFFIX_RE = /\s+(?:\d+\s+)?(?:(?:ultra|extra|semi|demi)[- ]?(?:light|bold)|thin|hairline|light|book|regular|normal|medium|black|heavy|bold)$/i;
const baseFamily = f => { const m = WEIGHT_SUFFIX_RE.exec(String(f).trim()); const b = m ? String(f).trim().slice(0, m.index).trim() : ''; return b || null; };

/** 글꼴 이름(또는 글)에서 글자 종류 */
function scriptOf(family, text) {
  const f = String(family || '');
  if (HANGUL_RE.test(f) || KO_NAME_RE.test(f)) return 'ko';
  if (KANA_RE.test(f) || JA_NAME_RE.test(f)) return 'ja';
  if (HAN_RE.test(f) || /simsun|yahei|jhenghei|pingfang|heiti|songti|mingliu|dengxian|kaiti|fangsong/i.test(f)) return 'zh';
  if (text) { if (HANGUL_RE.test(text)) return 'ko'; if (KANA_RE.test(text)) return 'ja'; if (HAN_RE.test(text)) return 'zh'; }
  return null;
}
const classOf = f => (MONO_RE.test(f) ? 'mono' : SERIF_RE.test(f) ? 'serif' : 'sans');

// ---------------- 글꼴 파일 → opentype 글꼴 ----------------
const TTC_TAG = 0x74746366;
/** .ttc 안의 글꼴 하나를 따로 떼어 낸다 (표 목록을 다시 쓰고 표 내용을 복사) — .ttf 는 그대로 */
function extractFace(buf, offset) {
  if (buf.readUInt32BE(0) !== TTC_TAG) return buf;
  const numTables = buf.readUInt16BE(offset + 4);
  let total = 12 + 16 * numTables;
  const entries = [];
  for (let t = 0; t < numTables; t++) {
    const e = offset + 12 + 16 * t;
    const tLen = buf.readUInt32BE(e + 12);
    entries.push({ dirPos: e, tOff: buf.readUInt32BE(e + 8), tLen, newOff: total });
    total += (tLen + 3) & ~3;
  }
  const out = Buffer.alloc(total);
  buf.copy(out, 0, offset, offset + 12);
  for (let t = 0; t < numTables; t++) {
    const e = entries[t];
    buf.copy(out, 12 + 16 * t, e.dirPos, e.dirPos + 8);
    out.writeUInt32BE(e.newOff, 12 + 16 * t + 8);
    out.writeUInt32BE(e.tLen, 12 + 16 * t + 12);
    buf.copy(out, e.newOff, e.tOff, e.tOff + e.tLen);
  }
  return out;
}

const parsed = new Map(); // 'path#offset' → opentype 글꼴 | null
const PARSED_MAX = 24;
function loadFace(face) {
  const k = `${face.path}#${face.offset}`;
  if (parsed.has(k)) return parsed.get(k);
  let font = null;
  try {
    const buf = extractFace(fs.readFileSync(face.path), face.offset);
    font = E.parseFont(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));
    if (!(font.unitsPerEm > 0) || typeof font.getAdvanceWidth !== 'function') font = null;
  } catch { font = null; }
  if (parsed.size >= PARSED_MAX) parsed.delete(parsed.keys().next().value);
  parsed.set(k, font);
  return font;
}

/** opentype 글꼴 → 엔진이 쓰는 모양 (unitsPerEm · ascender · descender · lineGap · getAdvanceWidth · charToGlyphIndex). 글자 하나씩 더해 어떤 글꼴에서도 안 죽게 */
function fontLike(font, familyForTweaks) {
  const tables = font.tables || {};
  let ascender = font.ascender, descender = font.descender, lineGap = tables.hhea && tables.hhea.lineGap > 0 ? tables.hhea.lineGap : 0;
  // Carlito(Calibri 대체) 의 hhea 는 1.0em 인데 PowerPoint 는 Calibri 를 OS/2 win 치수(1.22em)로 띄운다 — 이음오피스와 같은 처리
  const os2 = tables.os2;
  if (/^carlito/i.test(familyForTweaks || '') && os2 && os2.usWinAscent && os2.usWinDescent != null) { ascender = os2.usWinAscent; descender = -Math.abs(os2.usWinDescent); lineGap = 0; }
  const glyphIndex = ch => { try { return font.charToGlyphIndex(ch); } catch { return 0; } };
  return {
    unitsPerEm: font.unitsPerEm, ascender, descender, lineGap,
    charToGlyphIndex: glyphIndex,
    getAdvanceWidth(text, size, options) {
      const kern = !options || options.kerning !== false;
      try { return font.getAdvanceWidth(text, size, { kerning: kern }); } catch { /* 글꼴에 따라 커닝 표에서 죽는 것이 있다 → 글자 하나씩 */ }
      let units = 0, prev = null;
      for (const ch of text) {
        let g = null;
        try { g = font.charToGlyph(ch); } catch { g = null; }
        units += (g && g.advanceWidth) || 0;
        if (kern && prev && g) { try { units += font.getKerningValue(prev, g) || 0; } catch { /* 0 */ } }
        prev = g;
      }
      return (units / font.unitsPerEm) * size;
    },
  };
}

// ---------------- 이름 → 설치된 글꼴 ----------------
let index = null;
function fontIndex() { if (!index) { try { index = E.getFontIndex(); } catch { index = { byPs: new Map(), byFamily: new Map() }; } } return index; }
/** 글꼴 이름이 설치돼 있으면 그 이름의 글꼴 중 굵기·기울임에 맞는 것 */
function pickFace(family, bold, italic) {
  const faces = fontIndex().byFamily.get(key(family));
  if (!faces || !faces.length) return null;
  const want = [...(bold ? ['bold'] : []), ...(italic ? ['italic'] : [])];
  return [...faces].sort((a, b) => E.styleScore(b, want) - E.styleScore(a, want))[0];
}
/** 설치된 글꼴의 보이는 이름 (영어 이름표를 먼저) */
function displayNameOf(face, requested) {
  let fd = null;
  try {
    fd = fs.openSync(face.path, 'r');
    const tables = E.readTableDir(fd, face.offset);
    const name = tables && E.readTable(fd, tables, 'name', 1 << 20);
    if (name) {
      const count = name.readUInt16BE(2), strBase = name.readUInt16BE(4);
      let best = null, localized = null;
      for (let i = 0; i < count; i++) {
        const r = 6 + 12 * i;
        if (r + 12 > name.length) break;
        const platform = name.readUInt16BE(r), nameId = name.readUInt16BE(r + 6), lang = name.readUInt16BE(r + 4);
        if ((nameId !== 1 && nameId !== 16) || (platform !== 0 && platform !== 3)) continue;
        const len = name.readUInt16BE(r + 8), off = strBase + name.readUInt16BE(r + 10);
        if (off + len > name.length) continue;
        const s = Buffer.from(name.subarray(off, off + len)).swap16().toString('utf16le');
        if (!s) continue;
        if (lang === 0x409 || (platform === 0 && !best)) { if (!best || nameId === 16) best = s; } else if (!localized) localized = s;
      }
      if (best || localized) return best || localized;
    }
  } catch { /* 이름표를 못 읽으면 요청한 이름 */ } finally { if (fd != null) { try { fs.closeSync(fd); } catch { /* */ } } }
  return requested;
}

const resolved = new Map(); // 'family|BI|script' → { font, family, substituted } | null
/**
 * 글꼴 이름 + 굵게·기울임 (+ 글자 종류 힌트) → 쓸 글꼴. 없으면 null (→ 추정 메트릭)
 *   1 그 이름 · 다른 이름(별칭) · 굵기를 뗀 이름 → 설치된 것  2 글자 종류(한글·일본어·한자)에 맞는 대체 글꼴  3 세리프/고딕/고정폭 대체 글꼴
 */
function resolve(family, bold, italic, script) {
  const fam = String(family || '').split(',')[0].replace(/['"]/g, '').trim();
  const k = `${key(fam)}|${bold ? 1 : 0}${italic ? 1 : 0}|${script || ''}`;
  if (resolved.has(k)) return resolved.get(k);
  let out = null;
  const tryName = (name, substituted) => {
    const face = pickFace(name, bold, italic);
    if (!face) return false;
    const font = loadFace(face);
    if (!font) return false;
    const display = displayNameOf(face, name);
    out = { font: fontLike(font, display), family: display, substituted: !!substituted, path: face.path };
    return true;
  };
  const candidates = [fam, ...(ALIASES[key(fam)] || ALIASES[fam.toLowerCase()] || [])];
  const base = baseFamily(fam);
  if (base) candidates.push(base, ...(ALIASES[key(base)] || []));
  // 같은 이름 · 치수가 같은 짝 · 굵기를 뗀 이름은 '바꿈'이 아니다 (PowerPoint 도 그대로 커닝한다)
  const isTwin = c => key(c) === key(fam) || c === base || METRIC_TWINS.has(`${fam.toLowerCase()}>${c.toLowerCase()}`);
  for (const c of candidates) if (c && tryName(c, !isTwin(c))) break;
  if (!out) {
    const sc = script || scriptOf(fam);
    const serif = SERIF_RE.test(fam);
    const chains = [];
    if (sc === 'ko') chains.push(serif ? SUBST.koSerif : SUBST.ko, SUBST.ko);
    else if (sc === 'ja') chains.push(serif ? SUBST.jaSerif : SUBST.ja, SUBST.ja);
    else if (sc === 'zh') chains.push(SUBST.zh);
    chains.push(SUBST[classOf(fam)]);
    outer: for (const chain of chains) for (const c of chain) if (tryName(c, true)) break outer;
  }
  if (resolved.size > 4000) resolved.clear();
  resolved.set(k, out);
  return out;
}

/** 글(text)에 글꼴(font)이 없는 글자가 있나 (첫 CJK 글자만 본다 — 한글 글꼴이 아닌 글꼴로 한글을 재는 일을 막는다) */
function lacksScript(entry, text) {
  if (!entry || !text) return null;
  const probe = (HANGUL_RE.exec(text) || KANA_RE.exec(text) || HAN_RE.exec(text) || [])[0];
  if (!probe) return null;
  return entry.font.charToGlyphIndex(probe) === 0 ? scriptOf('', probe) : null;
}

/**
 * 엔진에 줄 글꼴 메트릭 공급자. opts.fonts === false 면 설치된 글꼴을 쓰지 않는다 (추정 메트릭만 — 점검용, 어느 PC 에서나 같은 결과)
 *   style = { fontFamily, fontSizePx, bold, italic, kerning, substScript, latinOnly }
 */
function metricsProvider(opts = {}) {
  const heuristic = new E.HeuristicMetrics();
  if (opts.fonts === false) return heuristic;
  const entryFor = (style, text) => {
    const script = style.substScript === 'ko' ? 'ko' : style.substScript === 'ja' ? 'ja' : style.substScript === 'sc' || style.substScript === 'tc' ? 'zh' : (style.latinOnly ? null : scriptOf('', text));
    let e = resolve(style.fontFamily, !!style.bold, !!style.italic, script);
    const missing = lacksScript(e, text);
    if (missing) { // 고른 글꼴에 이 글자가 없다 (예: Calibri 로 된 한글) → 그 글자 종류의 글꼴로
      const alt = resolve(`__script_${missing}`, !!style.bold, !!style.italic, missing);
      if (alt) e = { ...alt, substituted: true };
    }
    return e;
  };
  const ot = new E.OpentypeMetrics(style => { const e = entryFor(style, style.__text); return e ? e.font : undefined; }, heuristic);
  return {
    metrics(style) { return ot.metrics(style); },
    measure(text, style) { return ot.measure(text, { ...style, __text: text }); },
    displayFamily(style, text) { const e = entryFor(style, text); return e ? e.family : style.fontFamily; },
    substituted(style) { const e = entryFor(style, style.__text); return !!(e && e.substituted); },
  };
}

/** 점검·안내용: 이 PC 에서 글꼴 이름이 어떤 글꼴로 재어지는지 */
function describe(family, text) {
  const e = resolve(family, false, false, scriptOf(family, text));
  return e ? { family: e.family, substituted: e.substituted, path: e.path } : null;
}

module.exports = { metricsProvider, resolve, describe, scriptOf, baseFamily, extractFace, fontIndex, _reset() { resolved.clear(); parsed.clear(); } };
