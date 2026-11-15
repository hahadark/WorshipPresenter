// 원본(Electron) 조작 화면 소스에서 이벤트 처리기 줄을 뽑는다. 쓰는 법: node tools/behavior-parity/handlers.js <control 폴더> <나올 tsv>
const fs=require('fs');
const files=['control.js','cue.js','library.js','grid.js','slideedit.js','ctxmenu.js','cuemanager.js','dropfiles.js','paste.js','quicktext.js','menubar.js','mackeys.js'];
const re=/\.on(click|dblclick|contextmenu|keydown|keyup|input|change|wheel|paste|drop|dragstart|dragover|dragend|dragleave|dragenter|mousedown|mouseup|mousemove|pointerdown|pointermove|pointerup|blur|focus|scroll|submit|toggle|mouseenter|mouseleave|compositionend|compositionstart)\s*=|addEventListener\(\s*['"]([a-z]+)['"]/g;
const out=[];const cnt={};
for(const f of files){const ls=fs.readFileSync(require('path').join(process.argv[2],f),'utf8').split('\n');
 ls.forEach((l,i)=>{let m;re.lastIndex=0;while((m=re.exec(l))){const ev=m[1]||m[2];out.push(`${f}:${i+1}\t${ev}\t${l.trim().slice(0,230)}`);cnt[f]=(cnt[f]||0)+1;}});}
fs.writeFileSync(process.argv[3],out.join('\n'));console.log(cnt,out.length);
