// 시안 데이터에서 릴리스용 옮김 표와 Rust 대조용 기준표를 만든다.
// node docs/menu-2026-11/export.js (외부 패키지 없음)
const fs = require('fs');
const path = require('path');
const { F, PLANS } = require('./menus');
const split = s => { const m = s.match(/^(.*) \(([^()]*)\)$/); return m ? [m[1], m[2]] : [s, '']; };
const plan = PLANS.a.menus.map(([label, rows]) => ({label, items: rows.map(x => {
  if (x === '-') return {id:'-',label:'-',desc:'',key:''};
  if (typeof x === 'object' && !Array.isArray(x) && x.sub) return {id:x.sub,label:x.label,desc:'',key:'',sub:['c:lang-ko','c:lang-en']};
  const id = Array.isArray(x) ? x[0] : x;
  const [name, desc] = split(F[id][0]);
  return {id,label:Array.isArray(x)?x[1]:name,desc:Array.isArray(x)?(x[2]||''):desc,key:F[id][2]};
})}));
fs.writeFileSync(path.join(__dirname,'plan-a.json'),JSON.stringify(plan,null,2)+'\n');
const rows = Object.entries(F).map(([id,f]) => {
  const menu=plan.find(m=>m.items.some(i=>i.id===id || i.sub?.includes(id)));
  const item=menu.items.find(i=>i.id===id || i.sub?.includes(id));
  const old=id==='d:bulletin'?'파일 · 도구':f[3];
  const target=id.startsWith('c:lang-')?`보기 → Language · 언어 → ${id==='c:lang-ko'?'한국어':'English'}`:`${menu.label} → ${item.label}`;
  return `| ${f[0]} | ${old} | ${target} | ${item.desc||'—'} | ${f[2]||'—'} |`;
});
fs.writeFileSync(path.join(__dirname,'migration.md'),`# 메뉴 이동표 (다음 릴리스 설명용)\n\n파일 · 편집 · 보기 · 큐시트 · 화면 · 연결 · 송출 · AI · 도움말로 묶었습니다. 기존 69가지 동작과 단축키는 그대로이고, 두 자리에 있던 주보 만들기는 AI 메뉴 한 자리로 모았습니다.\n\n| 예전 항목 | 예전 메뉴 | 새 자리 · 이름 | 오른쪽 설명 | 단축키 |\n|---|---|---|---|---|\n${rows.join('\n')}\n`);
