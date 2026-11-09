'use strict';
// PPT 슬라이드를 '그대로' 그림으로 만든다
// - PowerPoint 가 설치돼 있으면 슬라이드마다 PNG 로 내보낸다 (.ppt .pps .pptx .ppsx 모두, 화면 그대로)
// - 없으면 .pptx/.ppsx 는 pptxbuild.js 가 이음오피스 PPTX 엔진(또는 직접 쓴 파서)으로 읽어 SVG 로 그린다 (그것도 안 되면 여기 extractPptx 로 글자·배경 그림만)

const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
const { execFile } = require('child_process');

let ppAvailable = null;
/** PowerPoint 설치 여부 (레지스트리의 PowerPoint.Application) */
function hasPowerPoint() {
  if (ppAvailable !== null) return Promise.resolve(ppAvailable);
  if (process.platform !== 'win32') return Promise.resolve(ppAvailable = false);
  return new Promise(res => execFile('reg', ['query', 'HKCR\\PowerPoint.Application\\CurVer'], { windowsHide: true }, err => res(ppAvailable = !err)));
}

const PS = `
$ErrorActionPreference = 'Stop'
$pp = New-Object -ComObject PowerPoint.Application
try {
  $p = $pp.Presentations.Open($env:WC_PPT_FILE, -1, 0, 0)
  $sw = $p.PageSetup.SlideWidth; $sh = $p.PageSetup.SlideHeight
  # (PowerShell 변수는 대소문자를 가리지 않는다)
  $outW = 1920; $outH = [int](1920 * $sh / $sw)
  $n = 0
  $texts = New-Object System.Collections.ArrayList
  # 슬라이드 글자: 글상자(그룹 안 포함)를 위에서 아래 순서로. 쪽 번호·바닥글·날짜 자리는 뺀다
  function Get-Texts($shapes) {
    $out = @()
    foreach ($sh in $shapes) {
      try {
        if ($sh.Type -eq 6) { $out += Get-Texts $sh.GroupItems; continue }
        if ($sh.Type -eq 14) { $pt = $sh.PlaceholderFormat.Type; if ($pt -eq 13 -or $pt -eq 15 -or $pt -eq 16) { continue } }
        if ($sh.HasTextFrame -and $sh.TextFrame.HasText) {
          $out += [pscustomobject]@{ top = [double]$sh.Top; left = [double]$sh.Left; text = [string]$sh.TextFrame.TextRange.Text }
        }
      } catch { }
    }
    return $out
  }
  foreach ($s in $p.Slides) {
    if ($s.SlideShowTransition.Hidden) { continue }
    $n++
    $s.Export((Join-Path $env:WC_PPT_OUT ('slide-{0:D3}.png' -f $n)), 'PNG', $outW, $outH)
    $parts = @(Get-Texts $s.Shapes | Sort-Object top, left | ForEach-Object { $_.text })
    [void]$texts.Add(($parts -join [char]10))
  }
  $json = ConvertTo-Json -InputObject @($texts) -Compress
  [IO.File]::WriteAllText((Join-Path $env:WC_PPT_OUT 'texts.json'), $json, (New-Object Text.UTF8Encoding $false))
  $p.Close()
  Write-Output "OK $n $outW $outH"
} finally {
  if ($pp.Presentations.Count -eq 0) { $pp.Quit() }
}
`;

// PPT 요소 따로 고치기 — PowerPoint 가 있을 때의 배경 그림: 파서(pptxbuild.js)가 꺼낸 요소(도형 id, XML 의 cNvPr id = COM 의 Shape.Id)를
//   슬라이드마다 숨기고 슬라이드를 PNG 로 내보낸다 → slide-NNN-bg.png (요소를 뺀 슬라이드가 픽셀 그대로). 문서화된 API 만 쓴다.
//   $env:WC_PPT_HIDE = hide.json { "1": ["2","5"], … } (보이는 슬라이드 번호 → 숨길 도형 id). 파일은 저장하지 않는다.
const PS_BG = `
$ErrorActionPreference = 'Stop'
$hide = Get-Content -LiteralPath $env:WC_PPT_HIDE -Raw -Encoding UTF8 | ConvertFrom-Json
$pp = New-Object -ComObject PowerPoint.Application
try {
  $p = $pp.Presentations.Open($env:WC_PPT_FILE, -1, 0, 0)
  $slideW = $p.PageSetup.SlideWidth; $slideH = $p.PageSetup.SlideHeight
  $outW = 1920; $outH = [int](1920 * $slideH / $slideW)
  function Get-Flat($shapes) {
    $out = New-Object System.Collections.ArrayList
    foreach ($x in $shapes) {
      try {
        [void]$out.Add($x)
        if ($x.Type -eq 6) { foreach ($y in (Get-Flat $x.GroupItems)) { [void]$out.Add($y) } }
      } catch { }
    }
    return ,$out
  }
  $n = 0
  foreach ($s in $p.Slides) {
    if ($s.SlideShowTransition.Hidden) { continue }
    $n++
    $ids = @()
    $prop = $hide.PSObject.Properties[[string]$n]
    if ($prop) { $ids = @($prop.Value | ForEach-Object { [string]$_ }) }
    if ($ids.Count -gt 0) {
      foreach ($x in (Get-Flat $s.Shapes)) {
        try { if ($ids -contains [string]$x.Id) { $x.Visible = 0 } } catch { }
      }
    }
    $s.Export((Join-Path $env:WC_PPT_OUT ('slide-{0:D3}-bg.png' -f $n)), 'PNG', $outW, $outH)
  }
  $p.Saved = -1
  $p.Close()
  Write-Output "OK $n $outW $outH"
} finally {
  if ($pp.Presentations.Count -eq 0) { $pp.Quit() }
}
`;

/** 큰 PPT 는 오래 걸린다: 1MB 마다 4초 더 (2분 ~ 30분) */
function pptTimeout(file) {
  let size = 0;
  try { size = fs.statSync(file).size; } catch { /* 없으면 PowerPoint 가 알린다 */ }
  return Math.max(120000, Math.min(1800000, 60000 + Math.round(size / 1048576) * 4000));
}

/** PowerShell 스크립트로 PowerPoint 를 부른다 → stdout 의 'OK n w h' */
function runPowerPoint(script, file, outDir, extraEnv = {}) {
  fs.mkdirSync(outDir, { recursive: true });
  const enc = Buffer.from(script, 'utf16le').toString('base64');
  const timeout = pptTimeout(file);
  return new Promise((resolve, reject) => {
    execFile('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', enc], {
      windowsHide: true, timeout,
      env: { ...process.env, WC_PPT_FILE: path.resolve(file), WC_PPT_OUT: path.resolve(outDir), ...extraEnv },
    }, (err, stdout, stderr) => {
      const m = String(stdout).match(/OK (\d+) (\d+) (\d+)/);
      if (err || !m) {
        const msg = String(stderr || (err && err.message) || '');
        if (/password|암호/i.test(msg)) return reject(new Error('암호가 걸린 파일이라 열 수 없습니다. PowerPoint 에서 암호를 풀고 저장해 주세요.'));
        if (err && err.killed) return reject(new Error(`PowerPoint 로 슬라이드를 만드는 데 ${Math.round(timeout / 60000)}분을 넘겼습니다. 파일이 너무 크면 PowerPoint 에서 '미디어 압축'으로 줄이거나 나눠 저장해 보세요.`));
        return reject(new Error('PowerPoint 로 슬라이드를 만들지 못했습니다. ' + (msg.split('\n').find(l => l.trim()) || '')));
      }
      resolve({ n: +m[1], width: +m[2], height: +m[3] });
    });
  });
}

/** PowerPoint 로 슬라이드를 PNG 로 내보낸다 → ['slide-001.png', …] */
async function exportWithPowerPoint(file, outDir) {
  const m = await runPowerPoint(PS, file, outDir);
  const files = fs.readdirSync(outDir).filter(f => /^slide-\d{3}\.png$/.test(f)).sort();
  // 슬라이드 글자 (PowerPoint 줄바꿈 \r · 세로탭 정리)
  let texts = [];
  try { texts = JSON.parse(fs.readFileSync(path.join(outDir, 'texts.json'), 'utf8').replace(/^﻿/, '')); } catch { /* 글자 없음 */ }
  if (!Array.isArray(texts)) texts = [texts];
  texts = texts.map(t => String(t || '').replace(/\r\n?|\u000b/g, '\n').replace(/\n{3,}/g, '\n\n').trim());
  return { files, texts, width: m.width, height: m.height };
}

/**
 * 요소 따로 고치기용 배경 그림 (PowerPoint): hide = { '1': [도형 id…], … } 의 도형을 숨기고 슬라이드마다 slide-NNN-bg.png
 * → { n } (hide.json 은 outDir 에 남는다)
 */
async function exportBgWithPowerPoint(file, outDir, hide) {
  fs.mkdirSync(outDir, { recursive: true });
  const hideFile = path.join(outDir, 'hide.json');
  fs.writeFileSync(hideFile, JSON.stringify(hide || {}));
  const m = await runPowerPoint(PS_BG, file, outDir, { WC_PPT_HIDE: path.resolve(hideFile) });
  return { n: m.n };
}

// ---------------- .pptx (zip) ----------------
function readZip(buf) {
  let eocd = -1;
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 70000); i--) if (buf.readUInt32LE(i) === 0x06054b50) { eocd = i; break; }
  if (eocd < 0) throw new Error('pptx 파일 형식이 아닙니다.');
  const count = buf.readUInt16LE(eocd + 10);
  let p = buf.readUInt32LE(eocd + 16);
  const entries = new Map();
  for (let k = 0; k < count && buf.readUInt32LE(p) === 0x02014b50; k++) {
    const method = buf.readUInt16LE(p + 10), csize = buf.readUInt32LE(p + 20);
    const nlen = buf.readUInt16LE(p + 28), xlen = buf.readUInt16LE(p + 30), clen = buf.readUInt16LE(p + 32);
    const off = buf.readUInt32LE(p + 42);
    const name = buf.subarray(p + 46, p + 46 + nlen).toString('utf8');
    entries.set(name, { method, csize, off });
    p += 46 + nlen + xlen + clen;
  }
  const get = name => {
    const e = entries.get(name);
    if (!e) return null;
    const start = e.off + 30 + buf.readUInt16LE(e.off + 26) + buf.readUInt16LE(e.off + 28);
    const raw = buf.subarray(start, start + e.csize);
    return e.method === 8 ? zlib.inflateRawSync(raw) : raw;
  };
  return { names: [...entries.keys()], get };
}

const xmlText = s => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&amp;/g, '&');

/** .pptx → { background, texts[][] } (PowerPoint 가 없을 때) */
function extractPptx(file) {
  const z = readZip(fs.readFileSync(file));
  const slides = z.names.filter(n => /^ppt\/slides\/slide\d+\.xml$/.test(n)).sort((a, b) => +a.match(/\d+/)[0] - +b.match(/\d+/)[0]);
  const texts = [];
  const used = new Map();
  for (const s of slides) {
    const xml = z.get(s).toString('utf8');
    const paras = [...xml.matchAll(/<a:p>([\s\S]*?)<\/a:p>/g)].map(m => [...m[1].matchAll(/<a:t>([^<]*)<\/a:t>/g)].map(t => xmlText(t[1])).join('')).filter(t => t.trim());
    texts.push(paras);
    const rels = z.get(s.replace('slides/', 'slides/_rels/') + '.rels');
    if (rels) for (const m of rels.toString('utf8').matchAll(/Target="\.\.\/media\/([^"]+\.(png|jpe?g))"/gi)) used.set(m[1], (used.get(m[1]) || 0) + 1);
  }
  // 슬라이드에 그림이 없으면 레이아웃·마스터(배경이 보통 여기에)
  if (!used.size) {
    for (const n of z.names.filter(x => /^ppt\/(slideLayouts|slideMasters)\/_rels\/.+\.rels$/.test(x))) {
      for (const m of z.get(n).toString('utf8').matchAll(/Target="\.\.\/media\/([^"]+\.(png|jpe?g))"/gi)) used.set(m[1], (used.get(m[1]) || 0) + 1);
    }
  }
  // 가장 많은 슬라이드에 쓰인 그림 = 배경
  let background = null;
  const best = [...used.entries()].sort((a, b) => b[1] - a[1])[0];
  if (best) background = { ext: /png$/i.test(best[0]) ? 'png' : 'jpg', data: z.get('ppt/media/' + best[0]) };
  return { background, scores: [], texts };
}

module.exports = { hasPowerPoint, exportWithPowerPoint, exportBgWithPowerPoint, extractPptx, readZip, PS, PS_BG };
