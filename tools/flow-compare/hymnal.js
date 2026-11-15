// 찬송가 찾기 · 베들레헴 찬송가(가사·교독문·악보) · 열 수 있는 파일 경계 대조: Electron 판 ↔ Rust 판 (그림을 찍지 않고 결과 값만 비교)
//
// 준비는 README.md 와 같다 (자료 두 벌 · Electron 소스 실행 · flow_driver).
// 실행: node tools\flow-compare\hymnal.js --el <포트> --rs-data tmp\rs-data --el-data tmp\el-data --out tmp\hymnal-out [--outside <등록 폴더 밖 PPT>]
//   --part search,bethlehem,scores,allow,layers 가운데 고르기 (기본 전부). layers = 붙여 넣은 PPT 의 요소 따로 꺼내기 결과
'use strict';
const fs = require('fs');
const path = require('path');
const { launch } = require('./cdp');

const A = process.argv.slice(2);
const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const EL = `http://127.0.0.1:${opt('el', '37528')}`;
const RSD = path.resolve(opt('rs-data', 'tmp/rs-data'));
const ELD = path.resolve(opt('el-data', 'tmp/el-data'));
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const RSCTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const OUT = opt('out', 'tmp/hymnal-out');
const PARTS = opt('part', 'search,bethlehem,scores,allow,layers').split(',');
const sleep = ms => new Promise(r => setTimeout(r, ms));
fs.mkdirSync(OUT, { recursive: true });

async function rsRaw(cmd, args = {}) {
  return (await fetch(RSCTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json();
}
async function rs(cmd, args = {}) {
  const r = await rsRaw(cmd, args);
  if (!r.ok) throw new Error(r.error);
  return r.result;
}
const sortKeys = v => (Array.isArray(v) ? v.map(sortKeys) : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map(k => [k, sortKeys(v[k])])) : v);
const J = v => JSON.stringify(sortKeys(v));
const lower = s => String(s || '').replace(/\//g, '\\').toLowerCase();

async function main() {
  const br = await launch(path.join(OUT, 'profile'));
  const report = { started: new Date().toISOString() };
  try {
    const ctl = await br.open(`${EL}/control/`, 1600, 950);
    if (!(await ctl.waitFor('!!(window.BCCue && window.BC && window.BC.state)', 30000))) throw new Error('Electron 조작 화면이 안 떴습니다');
    // BC.cmd 는 { ok, result | error } 를 돌려준다
    const el = async (n, a = {}) => {
      const r = await ctl.eval(`BC.cmd(${JSON.stringify(n)}, ${JSON.stringify(a)}).then(r => r, e => ({ ok: false, error: String(e && e.message || e) }))`, 120000);
      if (r && r.ok === false) throw new Error(r.error || '실패');
      return r && 'result' in r ? r.result : r;
    };

    if (PARTS.includes('search')) {
      const qs = ['', '1', '3', '03', '003', '010', '10', '27', '45', '455', '64', '645', '646', '700', '756', '999', '1000', '3장', '찬송가 3장', '찬송 27장', '교독문', '교독문 1', '교독문 3', '교독 3', '교독문3', '교독문 99',
        '예수', '주 예수', '거룩 거룩', '거룩거룩', '무한한', '구원', '군가', '군가 1', '시편', '나라 사랑', '아멘', 'abc', '  93  ', '만복'];
      const rep = (report.search = { total: qs.length, same: 0, diffs: [] });
      for (const q of qs) {
        const e = (await el('searchHymns', { query: q })).filter(h => !h.bethlehem).map(h => [lower(h.file), h.number ?? null, h.title, h.folder]);
        const r = (await rs('searchHymns', { query: q })).map(h => [lower(h.file), h.number ?? null, h.title, h.folder]);
        if (J(e) === J(r)) rep.same++;
        else {
          const es = new Set(e.map(x => x[0])), rset = new Set(r.map(x => x[0]));
          rep.diffs.push({ q, el: e.length, rs: r.length, sameSet: es.size === rset.size && [...es].every(x => rset.has(x)), elFirst: e.slice(0, 4).map(x => x[2]), rsFirst: r.slice(0, 4).map(x => x[2]), onlyEl: e.filter(x => !rset.has(x[0])).slice(0, 5).map(x => x[2]), onlyRs: r.filter(x => !es.has(x[0])).slice(0, 5).map(x => x[2]) });
        }
      }
      console.log(`찬송가 폴더 찾기: ${rep.same}/${rep.total} 같음`);
      for (const d of rep.diffs) console.log(`  '${d.q}': E${d.el}/R${d.rs}${d.sameSet ? ' (차례만 다름)' : ''} E=[${d.elFirst.join(' | ')}] R=[${d.rsFirst.join(' | ')}]${d.onlyEl.length ? ' E만: ' + d.onlyEl.join(' | ') : ''}${d.onlyRs.length ? ' R만: ' + d.onlyRs.join(' | ') : ''}`);
    }

    if (PARTS.includes('bethlehem')) {
      const eb = await el('hymnalBooks'), rb = await rs('hymnalBooks');
      report.books = { same: J(eb) === J(rb), el: eb, rs: rb };
      console.log(`베들레헴 책 목록: ${report.books.same ? '같음' : '다름 E=' + J(eb) + ' R=' + J(rb)}`);
      // 찾기
      const qs = ['1', '10', '10장', '645', '646', '교독문 1', '교독문 136', '교독문 137', '예수', '주 예수', '거룩', '만복의 근원', '시편', '나', 'ㄱ', '899', '900', '내 주를 가까이'];
      const sr = (report.bethSearch = { total: qs.length, same: 0, diffs: [] });
      for (const q of qs) {
        const e = (await el('searchHymns', { query: q })).filter(h => h.bethlehem).map(h => ({ book: h.book, number: h.number, title: h.title, label: h.label, kind: h.kind, scores: h.scores }));
        const r = (await rs('hymnalSearch', { query: q })).map(h => ({ book: h.book, number: h.number, title: h.kind === 'reading' ? h.label : h.title, label: h.label, kind: h.kind, scores: h.scores }));
        if (J(e) === J(r)) sr.same++; else sr.diffs.push({ q, el: e.length, rs: r.length, elFirst: e.slice(0, 3), rsFirst: r.slice(0, 3) });
      }
      console.log(`베들레헴 찾기: ${sr.same}/${sr.total} 같음`);
      for (const d of sr.diffs) console.log(`  '${d.q}': E${d.el}/R${d.rs} E=${J(d.elFirst).slice(0, 200)} R=${J(d.rsFirst).slice(0, 200)}`);
      // 한 장씩 꺼내기: 새찬송가 1~645 · 교독문 701~836 · 찬미가 1~899 · 없는 번호
      const gr = (report.bethGet = { total: 0, same: 0, bothError: 0, diffs: [] });
      const list = [];
      for (let n = 1; n <= 645; n++) list.push(['새찬송가', n]);
      for (let n = 701; n <= 836; n++) list.push(['새찬송가', n]);
      for (let n = 1; n <= 899; n++) list.push(['찬미가', n]);
      list.push(['새찬송가', 0], ['새찬송가', 650], ['새찬송가', 837], ['찬미가', 900], ['없는책', 1]);
      // Electron 쪽은 한 번에 여러 개씩 (CDP 왕복을 줄인다)
      for (let i = 0; i < list.length; i += 100) {
        const chunk = list.slice(i, i + 100);
        const es = await ctl.eval(`Promise.all(${JSON.stringify(chunk)}.map(([book, number]) => BC.cmd('hymnalGet', { book, number }).then(r => r, e => ({ ok: false, error: String(e && e.message || e) }))))`, 120000);
        for (let k = 0; k < chunk.length; k++) {
          const [book, number] = chunk[k];
          const e = es[k], r = await rsRaw('hymnalGet', { book, number });
          gr.total++;
          const eok = !(e && e.ok === false), rok = r.ok;
          if (!eok && !rok) { gr.bothError++; if (gr.bothError <= 8) (gr.errors = gr.errors || []).push({ book, number, el: e.error, rs: r.error }); continue; }
          const ev = eok ? ('result' in e ? e.result : e) : null, rv = rok ? r.result : null;
          if (eok && rok && J(ev) === J(rv)) { gr.same++; continue; }
          const ks = ev && rv ? [...new Set([...Object.keys(ev), ...Object.keys(rv)])].filter(x => J(ev[x]) !== J(rv[x])) : [];
          gr.diffs.push({ book, number, keys: ks, elError: eok ? undefined : e.error, rsError: rok ? undefined : r.error, el: Object.fromEntries(ks.map(x => [x, JSON.stringify(ev[x] ?? null).slice(0, 300)])), rs: Object.fromEntries(ks.map(x => [x, JSON.stringify(rv[x] ?? null).slice(0, 300)])) });
        }
      }
      console.log(`베들레헴 한 장 꺼내기: ${gr.same}/${gr.total} 같음 · 둘 다 오류 ${gr.bothError} · 다름 ${gr.diffs.length}`);
      for (const d of gr.diffs.slice(0, 12)) console.log(`  ${d.book} ${d.number}: ${d.keys.join(',')} ${d.elError ? 'E오류 ' + d.elError : ''} ${d.rsError ? 'R오류 ' + d.rsError : ''}\n    E=${J(d.el).slice(0, 260)}\n    R=${J(d.rs).slice(0, 260)}`);
    }

    if (PARTS.includes('scores')) {
      const crypto = require('crypto');
      const sc = (report.scores = { total: 0, same: 0, diffs: [] });
      const nums = [['새찬송가', 1], ['새찬송가', 3], ['새찬송가', 27], ['새찬송가', 93], ['새찬송가', 150], ['새찬송가', 304], ['새찬송가', 455], ['새찬송가', 588], ['새찬송가', 645], ['새찬송가', 646], ['새찬송가', 701], ['찬미가', 1], ['찬미가', 100], ['찬미가', 450], ['찬미가', 899], ['찬미가', 900]];
      for (const [book, n] of nums) {
        const u = `/hymnal/score/${encodeURIComponent(book)}/${n}.png`;
        const get = async base => { const r = await fetch(base + u); const b = Buffer.from(await r.arrayBuffer()); return { status: r.status, type: r.headers.get('content-type'), len: r.ok ? b.length : 0, sha: r.ok ? crypto.createHash('sha1').update(b).digest('hex').slice(0, 12) : '' }; };
        const [e, r] = [await get(EL), await get(RS)];
        sc.total++;
        if (e.status === r.status && e.sha === r.sha) sc.same++; else sc.diffs.push({ book, n, el: e, rs: r });
      }
      console.log(`베들레헴 악보 그림(/hymnal/score): ${sc.same}/${sc.total} 같음`);
      for (const d of sc.diffs) console.log(`  ${d.book} ${d.n}: E=${J(d.el)} R=${J(d.rs)}`);
    }

    if (PARTS.includes('allow')) {
      // 열 수 있는 파일 경계: 등록 폴더 · 붙여 넣은 폴더 · 끌어다 놓은 파일만
      const set = f => JSON.parse(fs.readFileSync(path.join(f, 'settings.json'), 'utf8').replace(/^\ufeff/, ''));
      const [es, rss] = [set(ELD), set(RSD)];
      const folder = (es.hymnFolders || [])[0];
      const first = folder && fs.readdirSync(folder.path).find(x => /\.pps$/i.test(x));
      const outside = opt('outside', '');
      const pasted = d => { const p = path.join(d, 'pasted'); const f = fs.existsSync(p) && fs.readdirSync(p).find(x => /\.pptx$/i.test(x)); return f ? path.join(p, f) : ''; };
      const cases = [];
      if (first) {
        cases.push(['등록 폴더 안', path.join(folder.path, first), path.join(folder.path, first)]);
        cases.push(['등록 폴더 안 (.. 을 거쳐 같은 파일)', path.join(folder.path, 'x', '..', first).replace(first, 'x\\..\\' + first).replace('x\\..\\x\\..\\', 'x\\..\\'), null]);
        cases.push(['등록 폴더 뒤 .. 로 밖의 파일', folder.path + '\\..\\' + path.basename(path.dirname(folder.path + '\\x')) + '-밖.pptx', null]);
        cases.push(['대소문자만 다른 경로', path.join(folder.path, first).toUpperCase(), null]);
      }
      if (outside) cases.push(['등록 폴더 밖 (끌어다 놓지 않은 파일)', outside, outside]);
      cases.push(['자기 자료의 붙여 넣은 PPT', pasted(ELD), pasted(RSD)]);
      cases.push(['다른 자료 폴더의 붙여 넣은 PPT', pasted(RSD), pasted(ELD)]);
      cases.push(['붙여 넣은 폴더 뒤 .. 로 설정 파일', path.join(ELD, 'pasted') + '\\..\\settings.json', path.join(RSD, 'pasted') + '\\..\\settings.json']);
      cases.push(['PPT 가 아닌 파일 (등록 폴더 밖)', path.join(ELD, 'settings.json'), path.join(RSD, 'settings.json')]);
      for (const d of (es.droppedFiles || []).slice(0, 2)) cases.push(['끌어다 놓은 파일', d, (rss.droppedFiles || []).includes(d) ? d : null]);
      const al = (report.allow = []);
      const denied = m => /등록한 찬송가 폴더|registered hymn folder/i.test(m || '');
      // 마지막에: 등록 폴더 밖 파일을 끌어다 놓은 것으로 등록한 뒤 다시 (두 판 다 열려야 한다)
      if (outside) cases.push(['등록 폴더 밖 파일을 끌어다 놓은 뒤', outside, outside, async () => {
        await el('addDroppedFiles', { paths: [outside] });
        await rs('updateSettings', { patch: { droppedFiles: [...(rss.droppedFiles || []), outside] } });
      }]);
      for (const [name, ef, rf0, before] of cases) {
        if (before) await before();
        const rf = rf0 === null ? ef : rf0;
        if (!ef || !rf) continue;
        let e, r;
        try { await el('loadHymn', { file: ef }); e = '열림'; } catch (x) { e = denied(x.message) ? '거절' : '오류: ' + x.message.slice(0, 80); }
        const rr = await rsRaw('loadHymn', { file: rf });
        r = rr.ok ? '열림' : denied(rr.error) ? '거절' : /만드는 중|being/i.test(rr.error) ? '열림(만드는 중)' : '오류: ' + rr.error.slice(0, 80);
        const same = e.slice(0, 2) === r.slice(0, 2);
        al.push({ name, el: e, rs: r, same, elFile: ef, rsFile: rf });
        console.log(`허용 경계 · ${name}: E ${e} / R ${r}${same ? '' : '  ← 다름'}`);
      }
    }

    if (PARTS.includes('layers')) {
      // 요소 따로 꺼내기: 자기 자료의 붙여 넣은 PPT (같은 이름) 마다 슬라이드별 요소(글상자·그림·도형)의 수·자리·글·크기·색을 견준다
      const names = fs.readdirSync(path.join(ELD, 'pasted')).filter(x => /^clip-\d{8}-\d{6}\.pptx$/i.test(x) && fs.existsSync(path.join(RSD, 'pasted', x)));
      const lr = (report.layers = []);
      const screen = { w: 1920, h: 1080 };
      const strip = it => { const o = { ...it }; delete o.id; if (typeof o.src === 'string') o.src = o.src.replace(/\/hymn\/[0-9a-f]{16}\//, '/hymn/K/').replace(/\?v=.*$/, ''); return o; };
      for (const name of names) {
        const rep1 = { name };
        lr.push(rep1);
        let e, r;
        try { e = await el('pptTextLayers', { file: path.join(ELD, 'pasted', name), screen }); } catch (x) { rep1.elError = x.message; }
        for (let i = 0; i < 240; i++) {
          const rr = await rsRaw('pptTextLayers', { file: path.join(RSD, 'pasted', name), screen });
          if (rr.ok) { r = rr.result; break; }
          if (!/만드는 중|being/i.test(rr.error)) { rep1.rsError = rr.error; break; }
          await sleep(250);
        }
        if (!e || !r) { console.log(`요소 꺼내기 · ${name}: ${rep1.elError ? 'E오류 ' + rep1.elError : ''} ${rep1.rsError ? 'R오류 ' + rep1.rsError : ''}`); continue; }
        rep1.slides = [e.slides.length, r.slides.length];
        rep1.bgs = [(e.bgs || []).filter(Boolean).length, (r.bgs || []).filter(Boolean).length];
        let items = 0, same = 0, near = 0; const diffs = [];
        for (let k = 0; k < Math.max(e.slides.length, r.slides.length); k++) {
          const [a, b] = [(e.slides[k] || []).map(strip), (r.slides[k] || []).map(strip)];
          if (a.length !== b.length) diffs.push({ slide: k + 1, count: [a.length, b.length], types: [a.map(x => x.type[0]).join(''), b.map(x => x.type[0]).join('')] });
          for (let j = 0; j < Math.min(a.length, b.length); j++) {
            items++;
            if (J(a[j]) === J(b[j])) { same++; continue; }
            const ks = [...new Set([...Object.keys(a[j]), ...Object.keys(b[j])])].filter(x => J(a[j][x]) !== J(b[j][x]));
            // 자리·크기가 화면의 0.2% 안, 글자 크기가 1 안이면 '거의 같음'
            const close = ks.every(x => typeof a[j][x] === 'number' && typeof b[j][x] === 'number' && Math.abs(a[j][x] - b[j][x]) <= (x === 'size' ? 1 : 0.2));
            if (close) near++; else diffs.push({ slide: k + 1, item: j, type: a[j].type, keys: ks, el: Object.fromEntries(ks.map(x => [x, a[j][x]])), rs: Object.fromEntries(ks.map(x => [x, b[j][x]])) });
          }
        }
        Object.assign(rep1, { items, same, near, diffs });
        console.log(`요소 꺼내기 · ${name}: 슬라이드 E${rep1.slides[0]}/R${rep1.slides[1]} · 배경 그림 E${rep1.bgs[0]}/R${rep1.bgs[1]} · 요소 ${items}개 가운데 같음 ${same} · 거의 같음 ${near} · 다름 ${diffs.length}`);
        for (const d of diffs.slice(0, 8)) console.log('   ' + JSON.stringify(d).slice(0, 330));
      }
    }
  } finally {
    fs.writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
    br.close();
  }
}

main().catch(e => { console.error(e); process.exit(1); });
