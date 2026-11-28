// 메뉴 안 A 실화면: node tools/menu/shots.js --seed tmp/menu-seed --cue tmp/menu-cue.json
// OS 키보드·포인터 조작 없이 앱의 점검 인자로 연다. 시험마다 자료 복사, 자기가 띄운 PID만 정리.
const fs=require('fs'), path=require('path'), {spawn}=require('child_process');
const arg=(k,d)=>{const i=process.argv.indexOf(k);return i<0?d:process.argv[i+1];};
const seed=path.resolve(arg('--seed','tmp/menu-seed')), cue=path.resolve(arg('--cue','tmp/menu-cue.json'));
const out=path.resolve(arg('--out','tmp/menu-captures')), exe=path.resolve(arg('--exe','target/debug/wp-app.exe'));
const only=arg('--only',''), jobs=Number(arg('--jobs','2'));
const names=['파일','편집','보기','큐시트','화면','연결','송출','AI','도움말'];
const cases=[];
for(const lang of ['ko','en'])for(const [size,scale] of [['1600x960',100],['1000x640',100],['1000x640',150]]) {
 for(let i=0;i<names.length;i++) cases.push({id:`${lang}-${size}-s${scale}-${i+1}`,lang,size,scale,menu:names[i]});
 if(scale===150)for(const [menu,sub] of [['보기','v:lang'],['화면','x:tpl-main'],['화면','x:tpl-overlay'],['연결','x:obs-url']])
  cases.push({id:`${lang}-${size}-s${scale}-${sub.replace(':','-')}`,lang,size,scale,menu,sub});
}
fs.mkdirSync(out,{recursive:true});
async function run(c) {
 const data=path.join(out,'data',c.id+'-'+Date.now());fs.mkdirSync(path.dirname(data),{recursive:true});fs.cpSync(seed,data,{recursive:true});
 const legacy=path.join(data,'empty-legacy');fs.mkdirSync(legacy,{recursive:true});
 const sp=path.join(data,'settings.json'), s=JSON.parse(fs.readFileSync(sp,'utf8'));
 Object.assign(s,{uiLang:c.lang,autoUpdate:false,telemetry:false,globalShortcuts:false,syncDir:''});
 for(const k of ['remote','lanOutput','phoneCam','webEditor','obs'])s[k]={...s[k],enabled:false};
 s.output={...s.output,autoOnSend:false,enabled:false};s.ui={...s.ui,scale:c.scale};fs.writeFileSync(sp,JSON.stringify(s));
 const a=['--data',data,'--win',c.size,'--import-cue',cue,'--open-menu',c.menu,'--menu-metrics',path.join(out,c.id+'.json'),'--shot',path.join(out,c.id+'.png')];
 if(c.sub)a.push('--open-sub',c.sub);
 const p=spawn(exe,a,{env:{...process.env,COMPUTERNAME:'TEST-PC',WP_LEGACY_DATA:legacy,WP_SHOT_AT:'4'},stdio:['ignore','ignore','pipe']});
 let log='';p.stderr.on('data',b=>log+=b);
 const timer=setTimeout(()=>p.kill(),40000);
 await new Promise(resolve=>p.on('exit',resolve));clearTimeout(timer);fs.writeFileSync(path.join(out,c.id+'.log'),log);
 if(!fs.existsSync(path.join(out,c.id+'.png')))throw new Error('캡처 없음: '+c.id);
 const m=JSON.parse(fs.readFileSync(path.join(out,c.id+'.json'),'utf8')), [x,y,r,b]=m.viewport;
 const top=m.items.filter(i=>i.kind==='top'), popup=m.items.filter(i=>i.kind==='popup'), sub=m.items.filter(i=>i.kind==='submenu');
 if(top.length!==9 || popup.length!==1 || (c.sub&&sub.length!==1) || m.items.some(i=>i.rect[0]<x-1||i.rect[1]<y-1||i.rect[2]>r+1||i.rect[3]>b+1))throw new Error('메뉴 범위: '+c.id+' '+JSON.stringify(m));
 console.log('ok',c.id,c.menu,c.sub||'',m.viewport);
}
(async()=>{const q=cases.filter(c=>!only||c.id.includes(only));await Promise.all(Array.from({length:jobs},async()=>{while(q.length)await run(q.shift());}));})();
