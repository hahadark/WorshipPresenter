// 검사: 안마다 지금 기능이 하나도 빠지지 않았는지, 메뉴별 줄 수 · 구분선 수
const { F, PLANS } = require('./menus.js');
const flat = (items, out = []) => { for (const x of items) { if (x === '-' ) continue; if (typeof x === 'object' && !Array.isArray(x) && x.h) continue; if (typeof x === 'object' && !Array.isArray(x) && x.sub) { if (x.items) flat(x.items, out); else out.push(x.sub); continue; } out.push(Array.isArray(x) ? x[0] : x); } return out; };
const O = x => typeof x === 'object' && !Array.isArray(x);
const count = items => ({ rows: items.filter(x => x !== '-' && !(O(x) && x.h)).length, seps: items.filter(x => x === '-').length, subs: items.filter(x => (O(x) && x.sub) || (typeof x === 'string' && F[x] && F[x][5] === 'sub')).length });
const need = Object.keys(F).filter(k => !k.startsWith('c:lang'));
for (const [k, p] of Object.entries(PLANS)) {
  const ids = p.menus.flatMap(([, it]) => flat(it)).map(i => i === 'v:lang' ? 'c:lang-ko' : i);
  const has = new Set(ids); if (ids.includes('c:lang-ko')) has.add('c:lang-en');
  if (has.has('x:url1')) has.add('x:obs-url');
  const miss = need.filter(i => !has.has(i));
  const dup = ids.filter((x, i) => ids.indexOf(x) !== i);
  console.log(k, 'menus', p.menus.length, 'rows', p.menus.reduce((a, [, it]) => a + count(it).rows, 0), 'miss', miss.join(',') || '-', 'dup', dup.join(',') || '-');
  console.log('   ' + p.menus.map(([n, it]) => { const c = count(it); return `${n} ${c.rows}/${c.seps}${c.subs ? '/▸' + c.subs : ''}`; }).join(' · '));
}
