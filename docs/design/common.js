// 2차 시안 공통: 예배 순서(1차와 같음) · 단순 선 아이콘(egui 로 선·원·다각형만으로 그릴 수 있는 것)
window.ITEMS = [
  { n: 1, t: '예배 전 찬양', s: '찬양팀', k: '제목', at: '11:00', m: 10 },
  { n: 2, t: '예배로 부름', s: '인도자', k: '제목', at: '11:10', m: 2 },
  { n: 3, t: '주님의 영광 나타나도다', s: '찬양', k: '찬양', at: '11:12', m: 5 },
  { n: 4, t: '교독문', s: '시 23', k: '성경', at: '11:17', m: 3 },
  { n: 5, t: '대표기도', s: '홍길동 정교', k: '공지', at: '11:20', m: 4 },
  { n: 6, t: '성경봉독', s: '요 15:1-8', k: '성경', at: '11:24', m: 3 },
  { n: 7, t: '설교', s: '포도나무 되신 주님', k: '제목', at: '11:27', m: 30 },
  { n: 8, t: '헌신 찬양', s: '내 모든 것 주께 드리네', k: '찬양', at: '11:57', m: 4 },
  { n: 9, t: '광고', s: '교회 소식', k: '공지', at: '12:01', m: 3 },
  { n: 10, t: '축도', s: '인도자', k: '제목', at: '12:04', m: 1 },
];
window.LIVE = 6; window.NEXT = 7;
window.VERSE = '무릇 내게 붙어 있어 열매를 맺지 아니하는 가지는 아버지께서 그것을 제거해 버리시고 무릇 열매를 맺는 가지는 더 열매를 맺게 하려 하여 그것을 깨끗하게 하시느니라';
const P = {
  sidebar: '<rect x="1.5" y="2.5" width="13" height="11" rx="2"/><line x1="6" y1="2.5" x2="6" y2="13.5"/>',
  inspector: '<rect x="1.5" y="2.5" width="13" height="11" rx="2"/><line x1="10" y1="2.5" x2="10" y2="13.5"/>',
  prev: '<polygon points="11,3 4,8 11,13" fill="currentColor" stroke="none"/><line x1="3.5" y1="3" x2="3.5" y2="13"/>',
  next: '<polygon points="5,3 12,8 5,13" fill="currentColor" stroke="none"/><line x1="12.5" y1="3" x2="12.5" y2="13"/>',
  play: '<polygon points="4,2.5 13.5,8 4,13.5" fill="currentColor" stroke="none"/>',
  clear: '<line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/>',
  hide: '<line x1="3" y1="3.5" x2="13" y2="3.5"/><line x1="8" y1="3.5" x2="8" y2="13"/><line x1="2" y1="14" x2="14" y2="2"/>',
  caption: '<rect x="1.5" y="3" width="13" height="10" rx="1.5"/><line x1="4" y1="7" x2="12" y2="7"/><line x1="4" y1="10" x2="9" y2="10"/>',
  search: '<circle cx="7" cy="7" r="4.5"/><line x1="10.3" y1="10.3" x2="14" y2="14"/>',
  monitor: '<rect x="1.5" y="2.5" width="13" height="9" rx="1"/><line x1="6" y1="14" x2="10" y2="14"/><line x1="8" y1="11.5" x2="8" y2="14"/>',
  timer: '<circle cx="8" cy="9" r="5.5"/><line x1="8" y1="9" x2="8" y2="5.5"/><line x1="6.5" y1="1.5" x2="9.5" y2="1.5"/>',
  add: '<line x1="8" y1="3" x2="8" y2="13"/><line x1="3" y1="8" x2="13" y2="8"/>',
  stage: '<rect x="1.5" y="3.5" width="13" height="9" rx="1"/><line x1="4" y1="6.5" x2="8" y2="6.5"/><circle cx="11.5" cy="9.5" r="1.3"/>',
  obs: '<circle cx="8" cy="8" r="6"/><circle cx="8" cy="8" r="2"/>',
  list: '<line x1="5" y1="4" x2="14" y2="4"/><line x1="5" y1="8" x2="14" y2="8"/><line x1="5" y1="12" x2="14" y2="12"/><circle cx="2.3" cy="4" r=".6"/><circle cx="2.3" cy="8" r=".6"/><circle cx="2.3" cy="12" r=".6"/>',
  grid: '<rect x="2" y="2" width="5" height="5"/><rect x="9" y="2" width="5" height="5"/><rect x="2" y="9" width="5" height="5"/><rect x="9" y="9" width="5" height="5"/>',
};
window.ic = (n, s = 16) => `<svg class="ic" width="${s}" height="${s}" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">${P[n]}</svg>`;
// 정적 HTML 의 <i data-ic="play"></i> 를 아이콘으로
document.addEventListener('DOMContentLoaded', () => document.querySelectorAll('[data-ic]').forEach(e => { e.outerHTML = ic(e.dataset.ic, +(e.dataset.s || 16)); }));
