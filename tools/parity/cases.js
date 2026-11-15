const R = 'theme-sa-redshield-main';
const notice = { kind: 'text', ref: '광고', label: '공지', parts: [{ book: 0, chapter: 0, verse: 0, text: '이번 주 **수요예배**는 저녁 7시 30분\n{색:#ffd479}교회 식당{/색}에서 모입니다\n\n> Wednesday service at 7:30 pm\n{크기:70}작은 안내 글{/크기}' }] };
const bible2 = { kind: 'bible', ref: '요한복음 3:16-17', title: '요한복음 3:16-18', parts: [
  { book: 43, chapter: 3, verse: 16, heading: '하나님의 사랑', text: '하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니 이는 그를 믿는 자마다 멸망하지 않고 영생을 얻게 하려 하심이라' },
  { book: 43, chapter: 3, verse: 17, text: '하나님이 그 아들을 세상에 보내신 것은 세상을 심판하려 하심이 아니요 그로 말미암아 세상이 구원을 받게 하려 하심이라', text2: 'For God did not send his Son into the world to condemn the world' }] };
const cells = [['left','top'],['center','top'],['right','top'],['left','middle'],['center','middle'],['right','middle'],['left','bottom'],['center','bottom'],['right','bottom']];
const cases = [];
for (const [a, v] of cells) cases.push({ name: `title-${a}-${v}`, item: 13, tpl: R, patch: { align: a, valign: v } });
cases.push({ name: 'bible-redshield', item: 6, slide: 0, tpl: R });
cases.push({ name: 'bible-long-center', slideJson: bible2, tpl: R, patch: { align: 'center', valign: 'middle', headings: { show: true, size: 36, color: '#cccccc' } }, versionName: '개역개정' });
cases.push({ name: 'bible-hanging', slideJson: bible2, tpl: R, patch: { align: 'hanging', valign: 'top' } });
cases.push({ name: 'bible-versebreak-left', slideJson: bible2, tpl: R, patch: { align: 'left', valign: 'middle', verseBreak: true, versionLabel: { show: true, size: 28, color: '#bbbbbb' } }, versionName: '개역개정' });
cases.push({ name: 'ref-bottom', slideJson: bible2, tpl: R, patch: { align: 'center', valign: 'middle', reference: { position: 'bottom', align: 'center' } } });
cases.push({ name: 'ref-fixed-top', slideJson: bible2, tpl: R, patch: { align: 'left', valign: 'middle', reference: { position: 'fixed-top', align: 'left' } } });
cases.push({ name: 'ref-fixed-bottom-safe', slideJson: bible2, tpl: R, patch: { safeArea: 5, align: 'center', valign: 'top', reference: { position: 'fixed-bottom', align: 'right', mode: 'passage' } } });
cases.push({ name: 'font-small-lh', slideJson: bible2, tpl: R, patch: { font: { size: 48, lineHeight: 1.2, weight: 700, autoFit: true }, align: 'center', valign: 'middle' } });
cases.push({ name: 'box-color', slideJson: bible2, tpl: 'worship', patch: { box: { enabled: true, color: '#000000', opacity: 0.6, padding: 32, radius: 16 }, valign: 'bottom' } });
cases.push({ name: 'notice-text', slideJson: notice, tpl: R, patch: { align: 'center', valign: 'middle' } });
cases.push({ name: 'notice-right-bottom', slideJson: notice, tpl: R });
cases.push({ name: 'logo-bottom-left-big', item: 13, tpl: R, patch: { logo: { position: 'bottom-left', size: 220, margin: 30, image: '/shared/assets/sa-crest.png' } } });
cases.push({ name: 'logo-top-center', item: 13, tpl: R, patch: { logo: { position: 'top-center', size: 90 } } });
cases.push({ name: 'bg-image-cover', item: 6, tpl: 'worship', patch: { background: { type: 'image', image: '/hymn/cad1989bcbe278f0/bg.png', imageFit: 'cover', dim: 0.3 } } });
cases.push({ name: 'bg-image-contain', item: 6, tpl: 'worship', patch: { background: { type: 'image', image: '/hymn/cad1989bcbe278f0/bg.png', imageFit: 'contain', color: '#203040', dim: 0 } } });
cases.push({ name: 'clean-lyrics-main', slideJson: bible2, tpl: 'theme-bc-clean-lyrics-main' });
cases.push({ name: 'full-band-main', slideJson: bible2, tpl: 'theme-bc-full-band-main' });
cases.push({ name: 'reading-big', item: 6, tpl: 'reading' });
cases.push({ name: 'image-ppt', item: 2, tpl: R });
cases.push({ name: 'score-hymn', item: 7, tpl: R });
cases.push({ name: 'blank-bg', item: 15, tpl: R });
cases.push({ name: 'blank-black', slideJson: { kind: 'blank', ref: '빈 화면', mode: 'black', parts: [] }, tpl: R });
cases.push({ name: 'logo-screen', item: 6, tpl: R, logo: true });
cases.push({ name: 'hide-bg', item: 6, tpl: R, hideBg: true });
cases.push({ name: 'footer-notice', slideJson: { ...notice, footer: '구세군대구사랑영문 · 주일 오전 11시' }, tpl: R, patch: { align: 'left', valign: 'middle' } });
cases.push({ name: 'edit-box', slideJson: { ...bible2, edit: { box: { x: 5, y: 55, w: 50, h: 35 }, style: { size: 48, color: '#ffe08a', align: 'left', valign: 'top', boxColor: '#000000', boxOpacity: 0.5 } } }, tpl: R });
// OBS 자막
cases.push({ name: 'obs-redshield-bible', item: 6, tpl: 'theme-sa-redshield-overlay', output: 'overlay' });
cases.push({ name: 'obs-gradient-box', slideJson: bible2, tpl: 'theme-bc-gradient-box-overlay', output: 'overlay' });
cases.push({ name: 'obs-full-band', slideJson: bible2, tpl: 'theme-bc-full-band-overlay', output: 'overlay' });
cases.push({ name: 'obs-stroke', slideJson: bible2, tpl: 'overlay', output: 'overlay' });
cases.push({ name: 'obs-score-lyrics', item: 7, tpl: 'overlay-lyrics', output: 'overlay' });
cases.push({ name: 'obs-image-caption', item: 5, tpl: 'theme-bc-clean-lyrics-overlay', output: 'overlay' });
cases.push({ name: 'obs-sermon-lower', item: 6, tpl: 'sermon-lower', output: 'overlay' });
// 그림 우상단·영상·카메라
cases.push({ name: 'obs-pip-image', item: 2, tpl: 'theme-sa-redshield-overlay', output: 'overlay', pip: true });
cases.push({ name: 'obs-pip-score', item: 7, tpl: 'overlay-lyrics', output: 'overlay', pip: true });
cases.push({ name: 'video-slide', slideJson: { kind: 'video', ref: '광고 영상', image: '/lib/file?p=C%3A%5Cnone.mp4', parts: [] }, tpl: R });
cases.push({ name: 'camera-slide', slideJson: { kind: 'camera', ref: '카메라', camera: { deviceId: '' }, parts: [] }, tpl: R });
require('fs').writeFileSync(process.argv[2] || 'tmp/cases-all.json', JSON.stringify(cases, null, 1));
console.log(cases.length);

