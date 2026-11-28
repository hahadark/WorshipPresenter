// 기능 대조표(docs/web/feature-matrix.md)에 빠진 메뉴 · 설정 항목이 있는지 본다.
//   node scripts/inventory.mjs        → 빠진 id 를 찍고, 있으면 1 로 끝남
// 분류는 하지 않는다 (분류와 까닭은 사람이 코드를 읽고 쓴다). 메뉴가 바뀌면 이 검사가 알려 준다.
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = p => readFileSync(join(root, p), 'utf8');

export function menuIds(src = read('crates/wp-app/src/menubar.rs')) {
  const body = src.slice(src.indexOf('pub fn menus()'), src.indexOf('pub fn title('));
  const ids = [...body.matchAll(/\b(?:it|itd|sub|dynsub)\("([^"]+)"/g)].map(m => m[1]).filter(id => id !== '-');
  return [...new Set(ids)];
}
export function settingIds(src = read('crates/wp-app/src/settings_items.rs')) {
  const body = src.slice(src.indexOf('pub static ITEMS'), src.indexOf('];', src.indexOf('pub static ITEMS')));
  return [...new Set([...body.matchAll(/\b(?:val|keep|link)\("([^"]+)"/g)].map(m => m[1]))];
}
/** 표에 `id` 로 적혀 있거나 `a:help-*` 같은 묶음으로 적혀 있으면 있는 것 */
export function missing(ids, doc = read('docs/web/feature-matrix.md')) {
  const listed = new Set([...doc.matchAll(/`([^`\s]+)`/g)].map(m => m[1]));
  const globs = [...listed].filter(x => x.endsWith('*')).map(x => x.slice(0, -1));
  return ids.filter(id => !listed.has(id) && !globs.some(g => id.startsWith(g)));
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const m = menuIds(), s = settingIds();
  const lost = [...missing(m).map(x => `메뉴 ${x}`), ...missing(s).map(x => `설정 ${x}`)];
  console.log(`메뉴 ${m.length}개 · 설정 ${s.length}개 확인`);
  if (lost.length) { console.log('대조표에 없는 것:\n  ' + lost.join('\n  ')); process.exit(1); }
  console.log('빠진 것 없음');
}
