'use strict';
// 클립보드 붙여 넣기 → 큐시트의 새 순서 (조작 화면 control/paste.js 가 부른다)
//  - 글: 어떤 순서로 넣을지 고른다 (장절 → 성경 · 가사 모양 → 찬양 · 그 밖 → 공지) — planText
//  - PowerPoint 에서 복사한 슬라이드 → .pptx 파일 (그 뒤는 끌어다 놓은 PPT 와 같다) — grabSlides
//      ① 클립보드의 슬라이드 묶음(.pptx 와 같은 zip)을 그대로 파일로 (Electron) ② 같은 것을 윈도우 .NET 으로
//      ③ PowerPoint 에 새 프레젠테이션을 만들어 붙여 넣고 저장 (원본 서식 유지 → 안 되면 그냥 붙여 넣기)
//    실제 PowerPoint 로는 아직 확인하지 못했다 — 그래서 방법마다 '정말 열리는지'(hymns.load) 보고 안 되면 다음 방법으로 간다
//  - 그림은 조작 화면이 /api/media 로 올린다 (여기 코드 없음)

const fs = require('fs');
const path = require('path');
const { execFile } = require('child_process');
const zlib = require('zlib');

// PowerPoint 가 슬라이드를 복사할 때 클립보드에 올리는 형식 이름
//  2026-11-10 실제 PowerPoint(Microsoft 365, 16.0)로 확인: 'PowerPoint 14.0 Slides Package' 가 슬라이드 묶음(zip, 뿌리 폴더가 ppt/ 대신 clipboard/)이고,
//  'PowerPoint 12.0 Internal Slides' 는 8바이트짜리 핸들일 뿐이다. 묶음은 clipToPptx 로 보통 .pptx 로 바꾼다
const SLIDE_FORMATS = ['PowerPoint 14.0 Slides Package', 'PowerPoint 12.0 Internal Slides', 'PowerPoint 14.0 Internal Slides', 'PowerPoint 15.0 Internal Slides', 'PowerPoint 16.0 Internal Slides'];

// ---------------- 글 ----------------
const MARK = /^\s*[[(（<]?\s*(\d+\s*절|후렴|브릿지|브리지|간주|전주|엔딩|verse\s*\d*|chorus\s*\d*|bridge|pre-?chorus|intro|outro|ending|tag)\s*[\])）>]?\s*[:.]?\s*$/i;
const ROLE = /^\s*(인도자|사회자|회중|다\s*같이|다함께|leader|people|all)\s*[:：]/i;
const short = (s, n = 40) => (s.length > n ? s.slice(0, n - 1).trimEnd() + '…' : s);

/** 붙여 넣은 글 정리 (줄 끝·앞뒤 빈 줄 · 2만 자까지) */
function cleanText(raw) {
  return String(raw ?? '').replace(/\r\n?/g, '\n').replace(/[ 　]/g, ' ').replace(/[​﻿]/g, '').replace(/[ \t]+$/gm, '').replace(/^\n+|\n+$/g, '').slice(0, 20000);
}

/** 가사 모양인가: 절 이름표(1절·후렴…)가 있거나, 짧은 줄 여러 개에 마침표·숫자가 거의 없다 */
function looksLikeLyrics(lines) {
  if (lines.some(l => ROLE.test(l))) return false; // 교독문은 공지 (인도자·회중 역할)
  if (lines.some(l => MARK.test(l))) return true;
  if (lines.length < 4) return false;
  if (lines.some(l => l.length > 36)) return false;
  const frac = re => lines.filter(l => re.test(l)).length / lines.length;
  return frac(/[.。!?]\s*$/) <= 0.1 && frac(/\d/) <= 0.2 && frac(/[:：]/) <= 0.1;
}

/**
 * 붙여 넣은 글 → 넣을 순서들
 *   as: '' (알아서) | 'bible' | 'song' | 'text'(공지) | 'title'
 *   isRef(줄) → 그 줄이 성경에서 찾아지는 장절인가
 * → { kind, items: [{ type, extra }] } | null (빈 글)
 */
function planText(raw, { as = '', isRef = () => false } = {}) {
  const text = cleanText(raw);
  if (!text.trim()) return null;
  const lines = text.split('\n').map(l => l.trim()).filter(Boolean);
  const first = lines[0];
  const refLines = lines.length <= 8 && lines.every(l => l.length <= 60 && /\d/.test(l) && isRef(l));
  const kind = ['bible', 'song', 'text', 'title'].includes(as) ? as : refLines ? 'bible' : looksLikeLyrics(lines) ? 'song' : 'text';
  if (kind === 'bible') {
    // 줄마다 장절 하나 (찾아지는 것만). 하나도 없으면 공지로
    const refs = lines.filter(l => l.length <= 60 && isRef(l)).slice(0, 8);
    if (refs.length) return { kind, items: refs.map(ref => ({ type: 'bible', extra: { title: '성경 봉독', ref } })) };
    return planText(text, { as: 'text', isRef });
  }
  if (kind === 'song') {
    // 첫 줄이 혼자 한 덩어리(다음이 빈 줄)면 제목, 아니면 첫 가사 줄을 제목으로
    const blocks = text.split(/\n[ \t]*\n+/);
    const head = blocks[0].trim();
    const alone = blocks.length > 1 && !head.includes('\n') && !MARK.test(head) && head.length <= 40;
    const title = alone ? head : short(lines.find(l => !MARK.test(l)) || first, 30);
    return { kind, items: [{ type: 'song', extra: { title, lyrics: alone ? blocks.slice(1).join('\n\n').trim() : text } }] };
  }
  if (kind === 'title') return { kind, items: [{ type: 'title', extra: { title: short(first, 60), ...(lines.length > 1 ? { subtitle: short(lines.slice(1).join(' '), 120) } : {}) } }] };
  // 공지: 짧은 첫 줄 + 내용 → 첫 줄이 제목. 한 줄뿐이면 제목만 (제목 카드로 보인다). 긴 글 한 덩어리는 제목 없이 (기본 '공지')
  if (lines.length === 1) return { kind, items: [{ type: 'text', extra: first.length <= 60 ? { title: first, body: '' } : { body: text } }] };
  const rest = text.slice(text.indexOf('\n') + 1).replace(/^\n+/, '');
  const headed = first.length <= 40 && !/[.。!?,]$/.test(first) && !ROLE.test(first);
  return { kind, items: [{ type: 'text', extra: headed ? { title: first, body: rest } : { body: text } }] };
}

// ---------------- PowerPoint 슬라이드 ----------------
// ---------------- 클립보드 슬라이드 묶음 → .pptx ----------------
let CRC_TABLE = null;
function crc32(buf) {
  if (typeof zlib.crc32 === 'function') return zlib.crc32(buf) >>> 0;
  if (!CRC_TABLE) { CRC_TABLE = new Uint32Array(256); for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; CRC_TABLE[n] = c >>> 0; } }
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
/** 간단한 zip 만들기: files = [{ name, data(Buffer) }] (deflate) */
function writeZip(files) {
  const locals = [];
  const centrals = [];
  let off = 0;
  for (const f of files) {
    const name = Buffer.from(f.name, 'utf8');
    const raw = Buffer.isBuffer(f.data) ? f.data : Buffer.from(f.data);
    const comp = zlib.deflateRawSync(raw);
    const crc = crc32(raw);
    const lh = Buffer.alloc(30);
    lh.writeUInt32LE(0x04034b50, 0); lh.writeUInt16LE(20, 4); lh.writeUInt16LE(0x0800, 6); lh.writeUInt16LE(8, 8);
    lh.writeUInt32LE(0, 10); lh.writeUInt32LE(crc, 14); lh.writeUInt32LE(comp.length, 18); lh.writeUInt32LE(raw.length, 22);
    lh.writeUInt16LE(name.length, 26); lh.writeUInt16LE(0, 28);
    const ch = Buffer.alloc(46);
    ch.writeUInt32LE(0x02014b50, 0); ch.writeUInt16LE(20, 4); ch.writeUInt16LE(20, 6); ch.writeUInt16LE(0x0800, 8); ch.writeUInt16LE(8, 10);
    ch.writeUInt32LE(0, 12); ch.writeUInt32LE(crc, 16); ch.writeUInt32LE(comp.length, 20); ch.writeUInt32LE(raw.length, 24);
    ch.writeUInt16LE(name.length, 28); ch.writeUInt32LE(off, 42);
    locals.push(lh, name, comp);
    centrals.push(ch, name);
    off += 30 + name.length + comp.length;
  }
  const cd = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(files.length, 8); end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(cd.length, 12); end.writeUInt32LE(off, 16);
  return Buffer.concat([...locals, cd, end]);
}
/**
 * 클립보드에서 읽은 묶음 → 보통 .pptx 버퍼 (이미 .pptx 면 그대로, 슬라이드 묶음이 아니면 null)
 * PowerPoint 의 'Slides Package' 는 뿌리가 clipboard/ 이고 프레젠테이션 종류가 매크로용(macroEnabled)이다 → ppt/ · 보통 종류로
 */
function clipToPptx(buf) {
  if (looksLikePptx(buf)) return buf;
  if (!Buffer.isBuffer(buf) || buf.length < 200 || buf[0] !== 0x50 || buf[1] !== 0x4b) return null;
  try {
    const { readZip } = require('./pptrender');
    const z = readZip(buf);
    if (!z.names.includes('clipboard/presentation.xml') || !z.names.some(n => /^clipboard\/slides\/slide\d+\.xml$/.test(n))) return null;
    const files = z.names.filter(n => !n.endsWith('/')).map(n => {
      let data = z.get(n);
      if (n === '[Content_Types].xml' || n === '_rels/.rels') {
        data = Buffer.from(data.toString('utf8')
          .replace(/PartName="\/clipboard\//g, 'PartName="/ppt/')
          .replace(/Target="(\/?)clipboard\//g, 'Target="$1ppt/')
          .replace(/presentationml\.presentation\.macroEnabled\.main\+xml|vnd\.ms-powerpoint\.presentation\.macroEnabled\.main\+xml/g, 'vnd.openxmlformats-officedocument.presentationml.presentation.main+xml'), 'utf8');
      }
      return { name: n.startsWith('clipboard/') ? 'ppt/' + n.slice(10) : n, data };
    });
    const out = writeZip(files);
    return looksLikePptx(out) ? out : null;
  } catch { return null; }
}

/** zip(.pptx) 모양인가: PK 로 시작하고 프레젠테이션·슬라이드가 들어 있다 */
function looksLikePptx(buf) {
  if (!Buffer.isBuffer(buf) || buf.length < 200 || buf[0] !== 0x50 || buf[1] !== 0x4b) return false;
  try {
    const { readZip } = require('./pptrender');
    const { names } = readZip(buf);
    return names.includes('[Content_Types].xml') && names.includes('ppt/presentation.xml') && names.some(n => /^ppt\/slides\/slide\d+\.xml$/.test(n));
  } catch { return false; }
}

// ② 윈도우 .NET: 클립보드의 슬라이드 묶음을 파일로 ($env:WC_CLIP_OUT). 'OK <바이트>' | 'HAVE'(형식은 있는데 내용을 못 읽음) | 'NONE'(슬라이드가 없음)
const PS_CLIP_DUMP = `
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$d = [System.Windows.Forms.Clipboard]::GetDataObject()
if (-not $d) { Write-Output 'NONE'; exit }
$all = @($d.GetFormats($false))
$f = @($all | Where-Object { $_ -match '^PowerPoint [0-9.]+ Slides Package$' }) + @($all | Where-Object { $_ -match '^PowerPoint [0-9.]+ Internal Slides$' }) | Select-Object -First 1
if (-not $f) { Write-Output 'NONE'; exit }
$s = $d.GetData($f, $false)
if ($s -is [System.IO.MemoryStream]) { $bytes = $s.ToArray() }
elseif ($s -is [byte[]]) { $bytes = $s }
else { Write-Output 'HAVE'; exit }
[System.IO.File]::WriteAllBytes($env:WC_CLIP_OUT, $bytes)
Write-Output ('OK ' + $bytes.Length)
`;

// ③ PowerPoint 에 붙여 넣어 저장 ($env:WC_PPT_FILE). 'OK <장수> keep|plain'
//   keep = 원본 서식 유지(리본의 '원본 서식 유지' 명령 — 창이 있어야 해서 새 창이 잠깐 뜬다), plain = 그냥 붙여 넣기(새 프레젠테이션의 기본 테마를 따른다)
//   문서화된 API 만: Presentations.Add · CommandBars.ExecuteMso · Slides.Paste · SaveAs(24 = .pptx). 사용자가 열어 둔 프레젠테이션은 건드리지 않는다
const PS_CLIP_PASTE = `
$ErrorActionPreference = 'Stop'
$pp = New-Object -ComObject PowerPoint.Application
$p = $null
try {
  $how = 'plain'
  try { $p = $pp.Presentations.Add(-1) } catch { $p = $pp.Presentations.Add(0) }
  try {
    $win = $p.Windows.Item(1)
    $win.Activate()
    $win.ViewType = 7
    $pp.CommandBars.ExecuteMso('PasteSourceFormatting')
    # 붙여 넣기는 PowerPoint 가 따로 처리한다: 슬라이드가 들어오고 장수가 더 늘지 않을 때까지 (5초까지)
    $last = -1
    for ($i = 0; $i -lt 50; $i++) {
      Start-Sleep -Milliseconds 100
      $c = $p.Slides.Count
      if ($c -gt 0 -and $c -eq $last) { break }
      if ($c -gt 0) { $last = $c }
    }
    if ($p.Slides.Count -gt 0) { $how = 'keep' }
  } catch { }
  if ($p.Slides.Count -eq 0) { [void]$p.Slides.Paste() }
  $n = $p.Slides.Count
  if ($n -eq 0) { throw 'no slides on the clipboard' }
  $p.SaveAs($env:WC_PPT_FILE, 24)
  Write-Output "OK $n $how"
} finally {
  if ($p) { try { $p.Saved = -1; $p.Close() } catch { } }
  if ($pp.Presentations.Count -eq 0) { $pp.Quit() }
}
`;

/** PowerShell 스크립트 실행 → stdout (윈도우만. -STA: 클립보드는 STA 스레드에서만 읽힌다) */
function runPs(script, env = {}, timeout = 60000) {
  const enc = Buffer.from(script, 'utf16le').toString('base64');
  return new Promise((resolve, reject) => {
    execFile('powershell.exe', ['-NoProfile', '-NonInteractive', '-STA', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', enc], { windowsHide: true, timeout, env: { ...process.env, ...env } }, (err, stdout, stderr) => {
      if (err) return reject(new Error(String(stderr || err.message || '').split('\n').find(l => l.trim()) || 'PowerShell 실행 실패'));
      resolve(String(stdout));
    });
  });
}

const pad = n => String(n).padStart(2, '0');
const stamp = (d = new Date()) => `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
/** 붙여 넣은 슬라이드를 두는 폴더 (hymns.isAllowed 가 이 안의 PPT 를 열게 해 준다) */
const pastedDir = dataDir => path.join(dataDir, 'pasted');

// 있을 수 없는 형식 이름: 이것까지 '있다'고 하면 형식 이름 확인(clipboard.has)을 믿을 수 없는 환경이다
const NO_SUCH_FORMAT = 'WorshipPresenter No Such Format 7f3a91';
let lastProbe = null; // { at, buf } — 방금 probeSlides 가 읽은 묶음 (바로 이어지는 grabSlides 가 큰 묶음을 다시 읽지 않게)

/**
 * 클립보드에 PowerPoint 슬라이드가 있는가 (Electron 에서만 빨리 알 수 있다)
 * → { slides: true(읽히는 .pptx 묶음이 있다) | 'maybe'(형식 이름만 보이고 내용은 못 읽음) | false, formatCheck: 형식 이름 확인을 믿을 수 있는가 }
 *   글·그림을 붙일 때마다 부르므로: 'maybe' 는 글이 없는 붙여 넣기에만 쓰고(paste.js), 형식 이름 확인이 믿을 만하지 않으면 아예 내지 않는다
 */
async function probeSlides(desktop) {
  lastProbe = null;
  if (!desktop || !desktop.clipboardBuffer) return { slides: false, formatCheck: false };
  // 데스크톱의 클립보드 함수는 비동기 (Electron 44) — 동기 함수(시험용)도 그대로 된다
  const has = async f => { try { return !!(desktop.clipboardHas && (await desktop.clipboardHas(f)) === true); } catch { return false; } };
  const formatCheck = !!desktop.clipboardHas && !(await has(NO_SUCH_FORMAT));
  let seen = false;
  for (const f of SLIDE_FORMATS) {
    if (formatCheck && !(await has(f))) continue; // 믿을 수 있으면 없는 형식은 읽지 않는다
    let buf = null;
    try { buf = await desktop.clipboardBuffer(f); } catch { buf = null; }
    const pptx = buf && buf.length ? clipToPptx(buf) : null;
    if (pptx) { lastProbe = { at: Date.now(), buf: pptx }; return { slides: true, formatCheck }; }
    if (formatCheck) seen = true;
  }
  return { slides: seen ? 'maybe' : false, formatCheck };
}
const hasSlides = async desktop => (await probeSlides(desktop)).slides;

/**
 * 클립보드의 PowerPoint 슬라이드 → <데이터 폴더>/pasted/clip-<날짜>-<시각>.pptx
 *   core: 코어 (desktop.clipboardBuffer · hymns.load · logger)
 *   opt.force: 슬라이드 형식이 안 보여도 PowerPoint 에 붙여 넣어 본다 (사용자가 'PowerPoint 슬라이드'를 직접 골랐을 때만 —
 *     그렇지 않으면 그림·글만 있는 클립보드를 PowerPoint 에 붙여 넣게 된다)
 *   opt.win · opt.ps(script, env) · opt.hasPowerPoint() 는 테스트에서 바꿔 넣는다
 * → { file, count, title, how: 'package' | 'keep' | 'plain' } · 슬라이드가 없거나 못 가져오면 Error
 */
async function grabSlides(core, opt = {}) {
  const win = opt.win ?? process.platform === 'win32';
  const ps = opt.ps || runPs;
  const hasPp = opt.hasPowerPoint || require('./pptrender').hasPowerPoint;
  const dir = pastedDir(core.dataDir);
  fs.mkdirSync(dir, { recursive: true });
  let seq = 0;
  const base = `clip-${stamp()}`;
  const next = () => { // 이미 있는 이름은 건너뛴다 (같은 초에 두 번 · 앞 방법이 남긴 것)
    for (;;) { const f = path.join(dir, `${base}${seq ? '-' + (seq + 1) : ''}.pptx`); seq++; if (!fs.existsSync(f)) return f; }
  };
  const log = m => { try { core.logger.info('슬라이드 붙여 넣기: ' + m); } catch { /* 기록만 */ } };
  /** 정말 열리는가 (끌어다 놓은 PPT 와 같은 길로) → 슬라이드 글 · 장수 */
  const usable = async (file, how) => {
    try {
      const meta = await core.hymns.load(file);
      const count = (meta.slides && meta.slides.length) || (meta.scores && meta.scores.length) || (meta.texts || []).filter(t => t && t.length).length;
      if (!count) throw new Error('슬라이드가 없습니다.');
      const firstText = String((meta.slideTexts && meta.slideTexts.find(t => String(t || '').trim())) || '').split('\n').map(l => l.trim()).find(Boolean) || '';
      log(`${how} → ${path.basename(file)} (${count}장)`);
      return { file, count, title: short(firstText, 30), how };
    } catch (e) {
      log(`${how} 로 만든 파일을 열지 못했습니다: ${e.message}`);
      try { fs.unlinkSync(file); } catch { /* 없으면 그만 */ }
      return null;
    }
  };
  let confirmed = !!opt.force; // 클립보드에 슬라이드가 정말 있는가 (③ 은 확인됐을 때만)
  // ① 클립보드의 슬라이드 묶음 그대로 (방금 probeSlides 가 읽어 둔 것이 있으면 그것)
  const d = core.desktop;
  const fresh = lastProbe && Date.now() - lastProbe.at < 10000 ? lastProbe.buf : null;
  lastProbe = null;
  if (d && d.clipboardBuffer) {
    for (const f of fresh ? [''] : SLIDE_FORMATS) {
      let buf = fresh;
      if (!buf) { try { buf = await d.clipboardBuffer(f); } catch { buf = null; } }
      if (!buf || !buf.length) continue;
      confirmed = true;
      const pptx = clipToPptx(buf);
      if (!pptx) { log(`'${f}' 은 있지만 .pptx 모양이 아닙니다 (${buf.length}바이트)`); continue; }
      const file = next();
      fs.writeFileSync(file, pptx);
      const r = await usable(file, 'package');
      if (r) return r;
    }
  }
  if (win) {
    // ② 같은 것을 .NET 으로 (스트림으로만 내주는 형식도 읽는다 · 형식이 있는지도 여기서 확실히 안다)
    try {
      const file = next();
      const out = await ps(PS_CLIP_DUMP, { WC_CLIP_OUT: file }, 30000);
      if (/^(OK \d+|HAVE)/m.test(out)) confirmed = true;
      if (/^OK \d+/m.test(out) && fs.existsSync(file)) {
        const pptx = clipToPptx(fs.readFileSync(file));
        if (pptx) { fs.writeFileSync(file, pptx); const r = await usable(file, 'package'); if (r) return r; }
        else { log('.NET 으로 읽은 묶음이 .pptx 모양이 아닙니다'); try { fs.unlinkSync(file); } catch { /* 그만 */ } }
      }
    } catch (e) { log('.NET 으로 읽지 못했습니다: ' + e.message); }
    if (!confirmed) throw new Error('클립보드에 PowerPoint 슬라이드가 없습니다. PowerPoint 의 슬라이드 목록에서 슬라이드를 골라 복사한 뒤 다시 붙여 넣으세요.');
    // ③ PowerPoint 에 붙여 넣어 저장
    if (await hasPp()) {
      const file = next();
      let out = '';
      try { out = await ps(PS_CLIP_PASTE, { WC_PPT_FILE: file }, 120000); } catch (e) { log('PowerPoint 에 붙여 넣지 못했습니다: ' + e.message); }
      const m = /OK (\d+) (keep|plain)/.exec(out);
      if (m && fs.existsSync(file)) { const r = await usable(file, m[2]); if (r) return r; }
    }
  }
  throw new Error('클립보드에서 PowerPoint 슬라이드를 가져오지 못했습니다. PowerPoint 에서 슬라이드를 다시 복사하거나, PPT 파일을 큐시트에 끌어다 놓으세요.');
}

module.exports = { planText, cleanText, looksLikeLyrics, looksLikePptx, clipToPptx, writeZip, grabSlides, hasSlides, probeSlides, pastedDir, SLIDE_FORMATS, PS_CLIP_DUMP, PS_CLIP_PASTE };
