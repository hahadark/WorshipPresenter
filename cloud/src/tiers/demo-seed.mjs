// 둘러보기(데모) 견본 교회의 처음 모습. 모두 지어낸 것이다 — 실제 번역본 · 찬송가 가사 · 교회 자료를 쓰지 않는다 (저작권 · 개인정보).
// 문서 모양은 설치판 JSON 모양을 따른다 (큐시트 {id, name, serviceDate, items[]} 등). 바꾸면 다음 되돌리기부터 그 모습이 된다.

const svg = (bg, fg, text) => `data:image/svg+xml;utf8,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${bg}"/><stop offset="1" stop-color="#101418"/></linearGradient></defs><rect width="1920" height="1080" fill="url(#g)"/><text x="960" y="560" font-size="96" text-anchor="middle" fill="${fg}" font-family="sans-serif">${text}</text></svg>`)}`;

export const DEMO_PICTURES = {
  welcome: svg('#2b4a6f', '#ffffff', '환영합니다 (견본 그림)'),
  offering: svg('#5b3a29', '#ffe9c7', '봉헌 (견본 그림)'),
};

export const DEMO_DOCS = [
  {
    kind: 'song', id: 'demo-song-1',
    payload: {
      id: 'demo-song-1', title: '견본 찬양 하나 (지어낸 가사)', author: '워십프리젠터 견본', key: 'G', bpm: 72, timeSig: '4/4',
      lyrics: '[1절]\n아침 햇살처럼 오시는 주님\n우리 마음 문을 두드리시네\n\n[후렴]\n주 이름 높이며 노래해\n오늘도 함께 걸어가리\n\n[2절]\n저녁 노을처럼 감싸시는 주님\n지친 걸음마다 쉬게 하시네',
      order: ['1절', '후렴', '2절', '후렴'],
    },
  },
  {
    kind: 'song', id: 'demo-song-2',
    payload: {
      id: 'demo-song-2', title: '견본 찬양 둘 (지어낸 가사)', author: '워십프리젠터 견본', key: 'D', bpm: 96, timeSig: '4/4',
      lyrics: '[1절]\n작은 손 모아 드리는 기도\n하늘에 닿아 빛이 되네\n\n[후렴]\n감사해 감사해\n모든 날 함께하심을',
      order: ['1절', '후렴', '후렴'],
    },
  },
  {
    kind: 'song', id: 'demo-song-3',
    payload: { id: 'demo-song-3', title: 'Sample Song (made-up lyrics)', author: 'WorshipPresenter sample', key: 'C', lyrics: '[Verse]\nMorning light is breaking\nEvery heart awaking\n\n[Chorus]\nSing a song of gladness\nSing it all day long', order: ['Verse', 'Chorus'] },
  },
  {
    kind: 'cue', id: 'demo-cue-1',
    payload: {
      id: 'demo-cue-1', name: '견본 주일 예배', serviceDate: '2026-11-29', startTime: '11:00', memo: '둘러보기용 견본 큐시트입니다. 마음껏 고쳐 보세요 — 하루 안에 처음 모습으로 돌아갑니다.', pinned: true, template: false,
      items: [
        { id: 'i1', type: 'image', title: '환영 화면', src: DEMO_PICTURES.welcome },
        { id: 'i2', type: 'title', title: '예배로 부름', text: '예배로 부름' },
        { id: 'i3', type: 'song', title: '견본 찬양 하나', songId: 'demo-song-1' },
        { id: 'i4', type: 'bible', title: '성경 봉독', ref: '요3:16-18', versionId: 'DEMO' },
        { id: 'i5', type: 'notice', title: '광고', text: '이번 주 소식 (견본)\n---\n수요 모임은 저녁 7시 (견본)' },
        { id: 'i6', type: 'song', title: '견본 찬양 둘', songId: 'demo-song-2' },
        { id: 'i7', type: 'image', title: '봉헌', src: DEMO_PICTURES.offering },
        { id: 'i8', type: 'blank', title: '빈 화면' },
      ],
    },
  },
  {
    kind: 'cue', id: 'demo-cue-2',
    payload: { id: 'demo-cue-2', name: '견본 수요 모임', serviceDate: '2026-12-02', startTime: '19:00', memo: '', pinned: false, template: false, items: [{ id: 'j1', type: 'bible', title: '시편 23편', ref: '시23', versionId: 'DEMO' }, { id: 'j2', type: 'song', title: 'Sample Song', songId: 'demo-song-3' }] },
  },
  {
    kind: 'settings', id: 'church',
    payload: { churchName: '견본 교회 (둘러보기)', bibleVersion: 'DEMO', note: '둘러보기 설정 — 하루 안에 처음 모습으로 돌아갑니다.' },
  },
];

/** 견본 성경: 장 · 절 수만 실제 성경과 같고 본문은 지어낸 글 (test/data-e2e/fixture.mjs 와 같은 모양) */
export const DEMO_BIBLE = { id: 'DEMO', name: '견본 성경 (지어낸 본문)', language: 'ko', licenseNote: '둘러보기용 — 실제 번역본이 아닙니다' };
export const DEMO_SHAPE = [[1, 1, 31], [19, 23, 6], [43, 1, 51], [43, 3, 36], [45, 8, 39]];
export const demoVerse = (b, c, v) => `(견본 본문) ${b}권 ${c}장 ${v}절 — 둘러보기를 위해 지어낸 글입니다.`;
