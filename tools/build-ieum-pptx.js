#!/usr/bin/env node
'use strict';
// 이음오피스(IeumOffice, Apache-2.0) 의 PPTX 엔진을 CommonJS 파일 하나로 묶는다 → src/core/vendor/ieum-pptx.js
//   node tools/build-ieum-pptx.js --src <이음오피스 소스 폴더>   (기본: ../ieumoffice 또는 환경 변수 IEUM_SRC)
// 묶이는 것: packages/pptx-engine(읽기 부분) · pptx-render(RenderTree · 글 배치 · 차트) · font-metrics(글꼴 찾기) · zip-gate
//           + 의존 패키지 jszip · fast-xml-parser · opentype.js · bidi-js (node_modules 의 devDependencies)
//   @genoffice/agent-core(AI 가림 표시 상수 3개) 는 아래 stub 으로 대신한다 — 그쪽 코드는 들어가지 않는다.
// 만든 파일은 저장소에 넣어 둔다 (실행할 때 빌드가 필요 없게). 이음오피스 소스를 새로 받아 묶으면 머리말의 커밋 번호가 바뀐다.
// 라이선스: 묶음 머리말에 Apache-2.0 안내 + 의존 패키지 라이선스, 같은 폴더에 LICENSE-ieumoffice.txt · NOTICE-ieumoffice.txt 를 복사한다.
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const esbuild = require('esbuild');

const ROOT = path.join(__dirname, '..');
const arg = n => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : undefined; };
const SRC = path.resolve(arg('--src') || process.env.IEUM_SRC || path.join(ROOT, '..', 'ieumoffice'));
const OUT_DIR = path.join(ROOT, 'src', 'core', 'vendor');
const OUT = path.join(OUT_DIR, 'ieum-pptx.js');

if (!fs.existsSync(path.join(SRC, 'packages', 'pptx-engine', 'src', 'index.ts'))) {
  console.error(`이음오피스 소스를 찾을 수 없습니다: ${SRC}\n  git clone https://github.com/pblsketch/ieumoffice 한 뒤 --src 로 폴더를 주세요.`);
  process.exit(1);
}
let commit = 'unknown';
try { commit = execFileSync('git', ['-C', SRC, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(); } catch { /* git 없음 */ }

const depVersion = name => { try { return require(path.join(ROOT, 'node_modules', name, 'package.json')).version; } catch { return '?'; } };
const DEPS = [
  ['jszip', 'MIT (또는 GPL-3.0-or-later, 둘 중 하나를 고를 수 있음)', 'LICENSE.markdown'],
  ['fast-xml-parser', 'MIT', 'LICENSE'],
  ['opentype.js', 'MIT', 'LICENSE'],
  ['bidi-js', 'MIT', 'LICENSE'],
];

// @ieum/<패키지>/… (들머리) 와 @genoffice/<패키지>[/하위] (이음오피스 안) 를 소스 파일로
const SUBPATHS = { // package.json "exports" 의 하위 경로 → 파일
  'pptx-engine': { custgeom: 'custgeom.ts', 'table-grid': 'table-grid.ts', identity: 'identity.ts', 'chart-workbook': 'chart-workbook.ts', model3d: 'model3d.ts', 'named-action': 'named-action.ts', 'background-promote': 'background-promote.ts' },
  'pptx-render': { 'preset-geometry': 'preset-geometry.ts', 'auto-num': 'auto-num.ts' },
  'font-metrics': { resolve: 'resolve.ts' },
};
const ieumPlugin = {
  name: 'ieum-src',
  setup(build) {
    build.onResolve({ filter: /^@ieum\// }, a => ({ path: path.join(SRC, 'packages', a.path.slice('@ieum/'.length)) }));
    build.onResolve({ filter: /^@genoffice\// }, a => {
      const [, pkg, ...rest] = a.path.split('/');
      const sub = rest.join('/');
      if (pkg === 'agent-core') return { path: 'agent-core-stub', namespace: 'stub' };
      if (pkg === 'font-metrics' && sub === 'word-faces.json') return { path: path.join(SRC, 'packages', 'font-metrics', 'word-faces.json') };
      const file = sub ? (SUBPATHS[pkg] || {})[sub] : 'index.ts';
      if (!file) throw new Error(`모르는 이음오피스 경로: ${a.path}`);
      return { path: path.join(SRC, 'packages', pkg, 'src', file) };
    });
    // agent-core 의 redact-range: 엔진이 쓰는 것은 가림 표시 XML 이름 상수 3개뿐
    build.onLoad({ filter: /.*/, namespace: 'stub' }, () => ({
      contents: `export const REDACT_NS = 'https://genspark.ai/genoffice/redaction/2026'; export const REDACT_PREFIX = 'go'; export const REDACT_EL = REDACT_PREFIX + ':redact';`,
      loader: 'js',
    }));
  },
};

const header = `/*!
 * ieum-pptx.js — 이음오피스(IeumOffice) PPTX 엔진 묶음 (워십프리젠터용, 자동 생성 — 고치지 마세요)
 *   원본: https://github.com/pblsketch/ieumoffice  커밋 ${commit}
 *         packages/pptx-engine · pptx-render · font-metrics · zip-gate (TypeScript → esbuild ${esbuild.version} 로 CommonJS 하나로)
 *   만든 날: ${new Date().toISOString().slice(0, 10)}  명령: node tools/build-ieum-pptx.js
 *
 * Copyright 2026 Mainfunc, Inc. (GenOffice) and the IeumOffice contributors.
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in
 * compliance with the License. You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed under the License is
 * distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and limitations under the License.
 * NOTICE: this file is a MODIFIED, BUNDLED build of part of IeumOffice (itself a modified GenOffice):
 *   only the pptx reading/rendering code is included, @genoffice/agent-core is replaced by a 3-constant stub,
 *   and the npm dependencies below are bundled in. Full license: LICENSE-ieumoffice.txt · NOTICE-ieumoffice.txt (same folder)
 *   and THIRD_PARTY_NOTICES.md at the repository root.
 *
 * Bundled dependencies (their own licenses, texts in LICENSES-ieum-pptx-deps.txt):
${DEPS.map(([n, l]) => ` *   ${n} ${depVersion(n)} — ${l}`).join('\n')}
 */
`;

(async () => {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  const r = await esbuild.build({
    entryPoints: [path.join(__dirname, 'ieum-pptx-entry.mjs')],
    outfile: OUT,
    bundle: true,
    platform: 'node',
    format: 'cjs',
    target: ['node22'],
    plugins: [ieumPlugin],
    nodePaths: [path.join(ROOT, 'node_modules')], // 이음오피스 소스 안의 import 'jszip' 등을 우리 node_modules 에서
    minifyWhitespace: true,
    minifySyntax: true,
    minifyIdentifiers: false, // 오류가 나면 함수 이름이 보이게
    legalComments: 'inline',
    sourcemap: false,
    logLevel: 'warning',
    banner: { js: header },
    define: { 'import.meta.url': '""' },
    metafile: true,
  });
  // 라이선스 글 복사
  fs.copyFileSync(path.join(SRC, 'LICENSE'), path.join(OUT_DIR, 'LICENSE-ieumoffice.txt'));
  fs.copyFileSync(path.join(SRC, 'NOTICE'), path.join(OUT_DIR, 'NOTICE-ieumoffice.txt'));
  let lic = `ieum-pptx.js 에 함께 묶인 npm 패키지의 라이선스 (node_modules 에서 복사, ${new Date().toISOString().slice(0, 10)})\n`;
  for (const [name, summary, file] of DEPS) {
    let text = '';
    try { text = fs.readFileSync(path.join(ROOT, 'node_modules', name, file), 'utf8'); } catch { text = '(라이선스 파일 없음)'; }
    lic += `\n${'='.repeat(78)}\n${name} ${depVersion(name)} — ${summary}\n${'='.repeat(78)}\n${text.trim()}\n`;
  }
  fs.writeFileSync(path.join(OUT_DIR, 'LICENSES-ieum-pptx-deps.txt'), lic);
  const size = fs.statSync(OUT).size;
  const inputs = Object.keys(r.metafile.inputs).length;
  console.log(`묶음: ${path.relative(ROOT, OUT)} ${(size / 1024).toFixed(0)}KB · 파일 ${inputs}개 · 이음오피스 ${commit.slice(0, 12)}`);
  // 바로 읽어 본다
  const m = require(OUT);
  for (const k of ['openPptx', 'buildRenderSlide', 'OpentypeMetrics', 'getFontIndex', 'parseFont']) if (typeof m[k] !== 'function') throw new Error(`묶음에 ${k} 가 없습니다`);
})().catch(e => { console.error(e); process.exit(1); });
