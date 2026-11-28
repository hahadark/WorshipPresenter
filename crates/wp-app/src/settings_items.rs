//! [설정 센터] 항목 목록 · 검색 · 한 칸만 고치기(patch) · 기본값
//!
//! 설정 화면은 여기 적힌 항목만 그린다. 값을 바꿀 때는 그 칸 하나만 코어에 써서,
//! 다른 창(템플릿 편집·출력 설정·리모컨 등)이 그 사이에 바꾼 값을 덮지 않는다.

use serde_json::{json, Value};
use wp_core::core::{Ev, Shared};
use wp_core::i18n::tr;

/// 왼쪽 분류 (원본 settings.js 의 탭 + Rust 판에서 도구 메뉴로 흩어진 것)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Cat {
    #[default]
    Send,
    Output,
    Obs,
    Ui,
    Keys,
    Data,
    Backup,
    Remote,
    About,
}

impl Cat {
    pub const ALL: [Cat; 9] = [Cat::Send, Cat::Output, Cat::Obs, Cat::Ui, Cat::Keys, Cat::Data, Cat::Backup, Cat::Remote, Cat::About];

    pub fn key(self) -> &'static str {
        ["send", "output", "obs", "ui", "keys", "data", "backup", "remote", "about"][self as usize]
    }

    pub fn from_key(k: &str) -> Option<Cat> {
        Cat::ALL.into_iter().find(|c| c.key() == k)
    }

    pub fn name(self) -> &'static str {
        ["송출 · 검색 · 분할", "예배당 출력", "OBS · 방송 자막", "조작 화면", "단축키", "성경 · 주석 데이터", "백업 · 동기화", "원격 · 장비 연결", "정보 · 업데이트"][self as usize]
    }

    /// 분류 제목 아래 한 줄 설명
    pub fn note(self) -> &'static str {
        [
            "송출 방식, 성경 검색, 슬라이드를 나누는 방법, 큐시트 넘기기를 정합니다.",
            "예배당 모니터에 나가는 화면과 그 화면에 쓰는 템플릿·로고를 정합니다.",
            "OBS 브라우저 소스 주소와 방송 자막 모양, OBS 원격 조작을 다룹니다.",
            "이 조작 화면의 색 테마, 글자 크기, 언어, 패널 배치를 정합니다.",
            "기능마다 키를 바꾸고, 겹치는 키를 찾아 고칩니다. 바꾼 키는 바로 적용됩니다.",
            "성경 번역본과 주석서를 가져오고 관리합니다.",
            "자료 백업과 여러 PC 동기화, 큐시트 내보내기를 다룹니다.",
            "휴대폰 리모컨, 스트림덱, Companion·음향·카메라 장비, 다른 PC 출력을 연결합니다.",
            "버전과 업데이트, 사용 통계, 라이선스 정보입니다.",
        ][self as usize]
    }
}

pub type Opts = &'static [(&'static str, &'static str)];

#[derive(Clone, Copy)]
pub enum Kind {
    /// 켜기·끄기
    Toggle,
    /// 고르기: [(값, 보일 이름)]
    Choice(Opts),
    /// 범위 막대 (추천값 단추 포함)
    Slider { lo: f64, hi: f64, step: f64, unit: &'static str, recommend: f64 },
    /// 정수
    Int { lo: i64, hi: i64 },
    /// 그림 파일: 고르기 · 지우기
    Image,
    /// 짧은 글
    Text,
    /// 출력별 템플릿 고르기 (activeTemplates 의 칸 이름)
    Template(&'static str),
    /// 다른 창 열기 (메뉴 id). 지금 상태 요약과 함께 보여 준다
    Link(&'static str),
    /// 출력 주소 복사 (주소 뒤쪽)
    Copy(&'static str),
    /// 설정 화면이 따로 그리는 것
    Special,
}

pub struct Item {
    /// 고정 이름 (검색 결과·시험·바로 가기)
    pub id: &'static str,
    pub cat: Cat,
    /// settings.json 안의 자리. 창 열기처럼 값이 없으면 빈 목록
    pub path: &'static [&'static str],
    pub label: &'static str,
    pub desc: &'static str,
    /// 검색에만 쓰는 관련 낱말
    pub words: &'static str,
    pub kind: Kind,
    /// 창 열기 단추 글
    pub btn: &'static str,
    /// '고급 설정' 접기 안에 넣는다
    pub advanced: bool,
    /// '이 분류를 기본값으로'에 포함한다 (파일·주소·언어·포트는 빼 둔다)
    pub reset: bool,
}

const fn val(id: &'static str, cat: Cat, path: &'static [&'static str], label: &'static str, desc: &'static str, words: &'static str, kind: Kind) -> Item {
    Item { id, cat, path, label, desc, words, kind, btn: "", advanced: false, reset: true }
}
/// 기본값 복원에서 빼는 값 (언어·파일·포트·템플릿 선택)
const fn keep(id: &'static str, cat: Cat, path: &'static [&'static str], label: &'static str, desc: &'static str, words: &'static str, kind: Kind) -> Item {
    Item { id, cat, path, label, desc, words, kind, btn: "", advanced: false, reset: false }
}
const fn link(id: &'static str, cat: Cat, menu: &'static str, label: &'static str, btn: &'static str, desc: &'static str, words: &'static str) -> Item {
    Item { id, cat, path: &[], label, desc, words, kind: Kind::Link(menu), btn, advanced: false, reset: false }
}
const fn adv(mut it: Item) -> Item {
    it.advanced = true;
    it
}

const SEARCH_MODES: Opts = &[("auto", "자동 (숫자가 있으면 장절, 없으면 본문)"), ("reference", "장절만"), ("text", "본문 단어만")];
const SPLIT_MODES: Opts = &[("auto", "화면 공간에 맞춰 자동"), ("count", "지정한 절 수만큼")];
const NAV_MODES: Opts = &[("range", "선택한 범위 안에서만 이동"), ("continuous", "다음(이전) 절로 계속 이어가기")];
/// crate::theme::ThemeId 가 실제로 가진 다섯 가지만 (예전 목록의 '만나로'는 '밝게'와 같은 색이라 뺐다)
pub const THEMES: Opts = &[("pro", "전문가 (기본)"), ("pro-light", "전문가 밝게"), ("dark", "어둡게"), ("contrast", "고대비"), ("light", "밝게"), ("warm", "따뜻하게"), ("navy", "푸른 밤")];
const LANGS: Opts = &[("ko", "한국어"), ("en", "English")];

pub static ITEMS: &[Item] = &[
    // ---------------- 송출 · 검색 · 분할 (원본 '기본' 탭) ----------------
    keep("songSearchUrl", Cat::Send, &["songSearchUrl"], "찬양 PPT 검색 주소", "{q} 자리에 검색어가 들어갑니다. 로그인과 다운로드는 브라우저에서 직접 합니다.", "찬양 PPT 온라인 검색 폴더 다운로드", Kind::Text),
    keep("songDownloadDir", Cat::Send, &["songDownloadDir"], "찬양 PPT 다운로드 폴더", "비우면 기본 다운로드 폴더를 지켜봅니다.", "찬양 PPT 온라인 검색 폴더 다운로드", Kind::Text),
    keep("songInboxDir", Cat::Send, &["songInboxDir"], "받은 PPT 모으는 곳", "비우면 기본 찬양 PPT 폴더로 옮깁니다.", "찬양 PPT 온라인 검색 폴더 다운로드", Kind::Text),
    val("previewMode", Cat::Send, &["previewMode"], "미리보기를 거쳐 송출", "켜면 누른 것이 먼저 미리보기에 올라가고 F9 로 송출합니다. 끄면 누른 것을 바로 송출합니다.", "프리뷰 바로 송출 즉시 preview", Kind::Toggle),
    val("searchMode", Cat::Send, &["searchMode"], "기본 검색 방식", "검색창에 쓴 글을 장절로 볼지, 본문 낱말로 볼지 정합니다.", "성경 찾기 장절 본문 낱말 search", Kind::Choice(SEARCH_MODES)),
    val("split.mode", Cat::Send, &["split", "mode"], "분할 방식", "성경 본문을 슬라이드 여러 장으로 나누는 방법입니다.", "슬라이드 나누기 절 묶기 split", Kind::Choice(SPLIT_MODES)),
    val("split.count", Cat::Send, &["split", "count"], "분할 절 수", "분할 방식이 '지정한 절 수만큼'일 때 한 장에 넣을 절 수입니다.", "슬라이드 나누기 몇 절씩", Kind::Int { lo: 1, hi: 20 }),
    val("navMode", Cat::Send, &["navMode"], "이동 (F5·F6 범위 끝에서)", "고른 범위의 마지막 절에서 F6 을 눌렀을 때 멈출지, 다음 절로 이어 갈지 정합니다.", "이전 다음 넘기기 계속", Kind::Choice(NAV_MODES)),
    val("cueAdvance", Cat::Send, &["cueAdvance"], "F6 을 누르면 다음 순서로 넘어가 바로 송출 (F5 는 이전 순서의 마지막 슬라이드)", "큐시트 순서의 마지막 슬라이드에서 F6 을 누르면 다음 순서를 이어서 송출합니다.", "큐시트 순서 끝 자동 넘김", Kind::Toggle),
    val("cueEdgeHide", Cat::Send, &["cueEdgeHide"], "그 전에 한 번은 글자만 숨기기 (모든 순서 — 순서마다 따로 켤 수도 있음)", "다음 순서로 넘어가기 전에 F6 한 번은 글자만 숨깁니다. 실수로 다음 순서가 나가는 것을 막습니다.", "큐시트 순서 끝 글자 숨김", Kind::Toggle),
    val("cueTimes", Cat::Send, &["cueTimes"], "예배 진행 시계 쓰기 (예배 시작 시각 · 순서별 예정 시간 · 몇 분 늦음)", "큐시트에 예배 시작 시각과 순서별 예정 시간을 적어 두고, 지금 얼마나 늦었는지 봅니다.", "시간 타이머 예정 늦음", Kind::Toggle),
    val("imeAuto", Cat::Send, &["imeAuto"], "가사·공지·즉석 자막 칸에 들어가면 한글 입력으로 (윈도우)", "글 쓰는 칸을 누르면 자판을 한글로 바꿔 줍니다.", "한영 전환 입력기 자판 IME", Kind::Toggle),
    // ---------------- 예배당 출력 (원본 '출력' 탭) ----------------
    link("out-extra", Cat::Output, "d:output", "출력 추가 · 소리 내는 출력", "출력 설정 열기", "로비 · 유아실 · 둘째 방송 같은 출력을 더하고, 출력마다 주소 · 모니터 · 기본 화면 구성 · 영상 소리를 정합니다.", "출력 더하기 로비 유아실 둘째 방송 화면 구성 소리 오디오 주소 모니터"),
    link("out-open", Cat::Output, "d:output", "예배당 화면 · 모니터", "출력 설정 열기", "예배당 화면을 켜고 끄기, 내보낼 모니터 고르기, 창으로 띄워 보기는 출력 설정 창에서 합니다.", "모니터 전체 화면 켜기 끄기 디스플레이 보조 화면 빔 프로젝터 창 모드"),
    val("output.autoOnSend", Cat::Output, &["output", "autoOnSend"], "송출하면 예배당 화면 자동 켜기", "예배당 화면이 꺼져 있어도 송출하는 순간 자동으로 켭니다.", "보조 화면 자동 켜짐", Kind::Toggle),
    val("output.cursorHidden", Cat::Output, &["output", "cursorHidden"], "예배당 화면에서 마우스 커서 숨김", "예배당 화면 위로 마우스가 넘어가도 화살표가 보이지 않습니다.", "마우스 포인터 화살표", Kind::Toggle),
    val("output.linkObs", Cat::Output, &["output", "linkObs"], "예배당 화면을 끄면 OBS 화면도 숨기기 (다시 켜거나 송출하면 다시 보임)", "예배당 화면과 방송 화면을 함께 끄고 싶을 때 켭니다.", "OBS 같이 끄기 연동", Kind::Toggle),
    keep("tpl-main", Cat::Output, &["activeTemplates", "main"], "예배당 화면 템플릿", "예배당 화면(과 OBS 전체 화면)에 쓰는 글꼴·배경·자리 묶음입니다.", "테마 글꼴 배경 디자인 모양", Kind::Template("main")),
    link("templates", Cat::Output, "c:templates", "출력 템플릿 편집", "템플릿 편집 열기", "글꼴·색·배경·여백·장절 표시·로고를 고칩니다.", "테마 글꼴 색 배경 여백 디자인 꾸미기"),
    link("themes", Cat::Output, "c:themes", "테마 고르기 (구세군 · 방송 스타일)", "테마 고르기 열기", "미리 만들어 둔 디자인 묶음에서 골라 템플릿으로 가져옵니다.", "테마 디자인 스타일 프리셋"),
    keep("logoScreenImage", Cat::Output, &["logoScreenImage"], "로고 화면 그림", "'로고' 단추를 눌렀을 때 화면 가득 보여 줄 그림입니다. 정하지 않으면 템플릿의 로고를 씁니다.", "로고 이미지 그림 파일 교회 마크", Kind::Image),
    // ---------------- OBS · 방송 자막 ----------------
    keep("url-main", Cat::Obs, &[], "예배당 화면 그대로", "OBS 브라우저 소스에 넣는 주소입니다. 예배당 화면과 똑같은 전체 화면이 나옵니다.", "주소 복사 브라우저 소스 URL 전체 화면", Kind::Copy("/outputs/main")),
    keep("url-overlay", Cat::Obs, &[], "투명 자막", "카메라 화면 위에 얹는 투명 자막 주소입니다.", "주소 복사 브라우저 소스 URL 자막 오버레이", Kind::Copy("/outputs/overlay")),
    keep("url-stage", Cat::Obs, &[], "무대 화면", "인도자·설교자가 보는 무대 화면 주소입니다.", "주소 복사 URL 스테이지 모니터", Kind::Copy("/outputs/stage")),
    keep("url-dock", Cat::Obs, &[], "OBS 조작 도킹", "OBS 사용자 지정 브라우저 독에 넣으면 송출을 조작할 수 있습니다.", "도킹 독 dock 조작 OBS 주소 URL", Kind::Copy("/dock")),
    keep("tpl-overlay", Cat::Obs, &["activeTemplates", "overlay"], "OBS 자막 템플릿 — 성경·공지", "투명 자막에서 성경과 공지에 쓰는 템플릿입니다.", "자막 모양 테마", Kind::Template("overlay")),
    keep("tpl-overlayLyrics", Cat::Obs, &["activeTemplates", "overlayLyrics"], "OBS 자막 템플릿 — 찬송가·찬양", "투명 자막에서 찬송가와 찬양 가사에 쓰는 템플릿입니다.", "자막 모양 테마 가사", Kind::Template("overlayLyrics")),
    keep("obsfull", Cat::Obs, &[], "OBS 자막: 슬라이드 그대로 (예배당 화면 모양)", "켜면 투명 자막 주소에도 자막 템플릿 대신 예배당 화면과 같은 슬라이드가 나갑니다. 프로그램을 다시 켜면 꺼집니다.", "자막 전체 슬라이드 그대로", Kind::Special),
    val("output.obsPip", Cat::Obs, &["output", "obsPip"], "OBS 자막: 그림 슬라이드를 오른쪽 위에 작게", "악보·그림 슬라이드를 방송 화면 구석에 작게 보여 줍니다.", "PIP 작은 화면 악보 그림", Kind::Toggle),
    link("obs-remote", Cat::Obs, "d:obs", "OBS 원격 조작", "OBS 원격 조작 열기", "OBS 에 연결해 장면 바꾸기, 방송·녹화 시작과 종료를 여기서 합니다. 주소와 비밀번호는 그 창에서 넣습니다.", "websocket 장면 방송 녹화 연결 비밀번호"),
    adv(link("lan", Cat::Obs, "d:lan", "내부망 출력 (다른 PC 의 OBS)", "내부망 출력 열기", "같은 네트워크의 다른 PC 에서 출력 화면을 열 수 있게 합니다. 포트와 접속 키는 그 창에서 정합니다.", "네트워크 LAN 다른 컴퓨터 송출 PC 포트")),
    adv(keep("port", Cat::Obs, &["port"], "로컬 서버 포트", "출력 주소에 쓰는 포트 번호입니다. 다른 프로그램과 겹칠 때만 바꾸세요. 프로그램을 다시 켜야 적용되고, OBS 에 넣어 둔 주소도 바꿔야 합니다.", "포트 번호 네트워크 주소 37528", Kind::Int { lo: 1024, hi: 65535 })),
    // ---------------- 조작 화면 ----------------
    val("ui.theme", Cat::Ui, &["ui", "theme"], "테마", "조작 화면의 색입니다. 예배당 화면 모양과는 상관없습니다.", "색 어둡게 밝게 다크 라이트 고대비 화면 색", Kind::Choice(THEMES)),
    val("ui.reduceMotion", Cat::Ui, &["ui", "reduceMotion"], "움직임 줄이기", "조절 칸과 구획이 바로 바뀝니다. 송출 화면의 전환에는 영향을 주지 않습니다.", "움직임 애니메이션 모션 접근성", Kind::Toggle),
    val("ui.scale", Cat::Ui, &["ui", "scale"], "글자 크기", "조작 화면의 글자와 단추 크기입니다. 막대를 놓으면 적용됩니다.", "확대 축소 크게 작게 배율 줌 폰트", Kind::Slider { lo: 70.0, hi: 200.0, step: 5.0, unit: "%", recommend: 100.0 }),
    keep("uiLang", Cat::Ui, &["uiLang"], "Language · 언어", "조작 화면과 출력 화면 안내 글의 언어입니다.", "한국어 영어 English Korean language", Kind::Choice(LANGS)),
    val("ui.directLiveOnly", Cat::Ui, &["ui", "directLiveOnly"], "바로 송출 모드에서는 자동으로 송출 뷰어만", "미리보기를 끄고 바로 송출할 때는 '다음' 뷰어가 쓸모없으니 송출 중 뷰어 하나만 보이고 본문·편집 자리를 넓힙니다. 미리보기 모드로 돌아오면 원래 고른 보기 방식으로 돌아갑니다.", "뷰어 하나 송출 중만 미리보기 프리뷰 프로그램 바로 송출 작업 공간 넓게", Kind::Toggle),
    link("workspaces", Cat::Ui, "w:workspaces", "작업공간 · 패널 배치", "작업공간 열기", "패널을 옮기고 크기를 바꾼 배치를 이름 붙여 저장하고 불러옵니다.", "레이아웃 패널 배치 도킹 창 배열"),
    // ---------------- 단축키 (목록은 settings_keys.rs 가 그린다) ----------------
    val("globalShortcuts", Cat::Keys, &["globalShortcuts"], "전역 단축키 (창이 안 보여도 F5·F6 등)", "OBS 같은 다른 프로그램을 쓰는 중에도 이전·다음·송출·숨김 네 가지 키가 듣습니다.", "글로벌 핫키 다른 프로그램 창 밖", Kind::Toggle),
    keep("globalkeys-status", Cat::Keys, &[], "전역 단축키 등록 상태", "", "글로벌 핫키 등록 실패", Kind::Special),
    val("reservedKeys", Cat::Keys, &["reservedKeys"], "다른 프로그램 키 (쉼표로)", "OBS 나 다른 예배 프로그램이 쓰는 키를 적어 두면, 같은 키를 정할 때 알려 줍니다. 예: F1, Ctrl+Shift+S", "충돌 방지 예약 키 OBS 핫키", Kind::Text),
    // ---------------- 성경 · 주석 데이터 ----------------
    link("bible", Cat::Data, "c:import-bible", "성경 번역본", "번역본 가져오기 · 관리 열기", "성경 파일을 분석해 가져오고, 베들레헴 성경에서 한꺼번에 가져오고, 번역본을 지웁니다.", "번역본 개역개정 새번역 가져오기 삭제 베들레헴 import bible"),
    link("commentary", Cat::Data, "c:commentary", "주석서 (조작 화면 참고용 · 송출되지 않음)", "주석 보기 · 가져오기 열기", "EPUB 주석서를 가져오고, 보고 있는 본문의 주석을 조작 화면에서만 읽습니다.", "주석 EPUB 참고 자료 commentary"),
    val("webConsent", Cat::Data, &["webConsent"], "인터넷에서 가사 · PPT 찾기 쓰기 (저작권 안내에 동의함)", "가사 찾기(벅스 · 지니 · 멜론)와 온라인 찬양 PPT 찾기를 씁니다. 끄면 인터넷에 묻지 않고, 다음에 쓸 때 저작권 · CCLI 안내를 다시 보이고 동의를 받습니다.", "인터넷 가사 PPT 저작권 CCLI 동의 웹", Kind::Toggle),
    // ---------------- 백업 · 동기화 ----------------
    link("sync", Cat::Backup, "b:sync", "여러 PC 동기화 · 자동 백업", "동기화 · 백업 열기", "동기화 폴더 정하기, 지금 백업, 백업에서 되돌리기를 합니다.", "백업 복원 되돌리기 원드라이브 구글 드라이브 폴더 OneDrive sync backup"),
    link("cues", Cat::Backup, "b:cues", "큐시트 관리 (휴지통 · 복제 · 내보내기)", "큐시트 관리 열기", "큐시트를 파일로 내보내고 가져오고, 지운 큐시트를 휴지통에서 되살립니다.", "큐시트 내보내기 가져오기 휴지통 복구 export"),
    link("usage", Cat::Backup, "c:usage", "사용 기록 (저작권 보고)", "사용 기록 열기", "송출한 찬양 기록을 기간별로 모아 저작권 보고용으로 내보냅니다.", "CCLI 저작권 보고 기록 통계"),
    // ---------------- 원격 · 장비 연결 ----------------
    link("remote", Cat::Remote, "d:remote", "휴대폰 리모컨", "휴대폰 리모컨 열기", "같은 와이파이의 휴대폰으로 슬라이드를 넘깁니다. 켜기·끄기와 PIN, QR 코드는 그 창에 있습니다.", "모바일 폰 리모트 QR PIN 와이파이 remote"),
    link("trigger", Cat::Remote, "d:trigger", "스트림덱 · 외부 단추", "스트림덱 · 외부 단추 열기", "스트림덱이나 다른 프로그램에서 주소를 불러 송출 동작을 실행합니다.", "stream deck 트리거 API 단추 주소 보안 키"),
    link("devices", Cat::Remote, "d:devices", "Companion · 장비 (X32 · PTZ)", "장비 연결 열기", "Companion, 음향 콘솔(X32), PTZ 카메라를 등록하고 순서에 맞춰 움직이게 합니다.", "컴패니언 믹서 음향 카메라 프리셋 OSC VISCA"),
    link("cameras", Cat::Remote, "b:cameras", "카메라 설정", "카메라 설정 열기", "카메라·캡처 장치에 이름을 붙이고 큐시트 순서로 넣습니다.", "웹캠 캡처 보드 영상 입력"),
    link("lan2", Cat::Remote, "d:lan", "내부망 출력 (다른 PC 의 OBS)", "내부망 출력 열기", "송출 PC 가 따로 있을 때 그 PC 의 OBS 에서 이 프로그램의 출력 화면을 엽니다.", "네트워크 LAN 다른 컴퓨터 송출 PC"),
    link("diagnose", Cat::Remote, "b:diagnose", "연결 점검 (예배 전 점검)", "연결 점검 열기", "출력 화면, OBS, 장비, 동기화 폴더가 제대로 이어져 있는지 한 번에 살핍니다.", "진단 테스트 확인 문제 해결"),
    // ---------------- 정보 · 업데이트 ----------------
    keep("version", Cat::About, &[], "지금 버전", "", "버전 version 릴리스 건너뛰기", Kind::Special),
    val("autoUpdate", Cat::About, &["autoUpdate"], "켤 때 새 버전 확인", "프로그램을 켤 때 새 버전이 나왔는지 한 번 확인합니다.", "자동 업데이트 업그레이드 update", Kind::Toggle),
    val("telemetry", Cat::About, &["telemetry"], "익명 사용 통계 보내기 (하루 한 번: 무작위 설치 번호 · 버전 · 언어 · OS)", "어느 교회인지, 무엇을 송출했는지는 보내지 않습니다.", "통계 개인정보 수집 telemetry", Kind::Toggle),
    link("update", Cat::About, "d:update", "새 버전 확인", "업데이트 창 열기", "지금 새 버전이 있는지 확인하고 내려받습니다.", "업데이트 내려받기 설치 update"),
    link("about", Cat::About, "c:about", "정보 · 라이선스", "정보 · 라이선스 열기", "성경 데이터 출처, 저작권 안내, 프로그램과 공개 라이브러리 라이선스입니다.", "저작권 라이선스 GPL 출처 about"),
    link("help-keys", Cat::About, "a:help-keys", "단축키 보기", "단축키 표 열기", "지금 쓰는 단축키를 한 장으로 봅니다.", "도움말 단축키 표"),
];

pub fn find(id: &str) -> Option<&'static Item> {
    ITEMS.iter().find(|i| i.id == id)
}

/// 경로의 값 (없으면 Null)
pub fn read<'a>(s: &'a Value, path: &[&str]) -> &'a Value {
    let mut v = s;
    for p in path {
        v = &v[*p];
    }
    v
}

pub fn default_of(path: &[&str]) -> Value {
    read(&wp_core::settings::DEFAULTS, path).clone()
}

/// 그 칸 하나만 고친다 (다른 칸은 읽지도 쓰지도 않는다). 고치기 전 값을 돌려준다
pub fn patch(core: &Shared, path: &[&str], v: Value) -> Value {
    let mut c = core.lock();
    let old = read(c.settings.get(), path).clone();
    if old == v || path.is_empty() {
        return old;
    }
    let nv = v.clone();
    c.settings.update(|d| {
        let mut cur = d;
        for p in path {
            if !cur.is_object() {
                *cur = json!({});
            }
            cur = &mut cur[*p];
        }
        *cur = nv;
    });
    if path == ["uiLang"] {
        if let Some(l) = v.as_str() {
            wp_core::i18n::set_lang(l);
        }
    }
    c.emit(Ev::Live); // 출력 웹 화면(커서·자막·언어)도 바로 반영
    c.emit(Ev::Ui);
    old
}

/// 검색: 띄어 쓴 낱말이 모두 이름·설명·관련 낱말·분류 이름(한국어·지금 화면 언어) 어딘가에 있으면 맞음
pub fn matches(it: &Item, q: &str) -> bool {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return false;
    }
    let hay = format!("{} {} {} {} {} {} {} {} {}", it.label, tr(it.label), it.desc, tr(it.desc), it.words, it.cat.name(), tr(it.cat.name()), it.btn, tr(it.btn)).to_lowercase();
    q.split_whitespace().all(|w| hay.contains(w))
}

pub fn search(q: &str) -> Vec<&'static Item> {
    ITEMS.iter().filter(|i| matches(i, q)).collect()
}

/// 값을 사람이 읽는 글로 (지금 값·기본값 표시)
pub fn value_text(it: &Item, v: &Value) -> String {
    match it.kind {
        Kind::Toggle => tr(if v.as_bool().unwrap_or(false) { "켬" } else { "끔" }),
        Kind::Choice(opts) => {
            let s = v.as_str().unwrap_or("");
            opts.iter().find(|o| o.0 == s).map(|o| tr(o.1)).unwrap_or_else(|| s.to_string())
        }
        Kind::Slider { unit, .. } => format!("{}{}", v.as_f64().unwrap_or(0.0).round() as i64, unit),
        Kind::Int { .. } => v.as_i64().map(|n| n.to_string()).unwrap_or_default(),
        _ => match v.as_str() {
            Some("") | None => tr("비어 있음"),
            Some(s) => s.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 항목_이름은_겹치지_않고_값_항목은_기본값이_있다() {
        let mut seen = std::collections::BTreeSet::new();
        for it in ITEMS {
            assert!(seen.insert(it.id), "항목 이름이 겹침: {}", it.id);
            if !it.path.is_empty() {
                assert!(!default_of(it.path).is_null(), "기본값이 없는 설정 칸: {} ({:?})", it.id, it.path);
            }
            if it.reset {
                assert!(matches!(it.kind, Kind::Toggle | Kind::Choice(_) | Kind::Slider { .. } | Kind::Int { .. } | Kind::Text), "기본값 복원에 넣으면 안 되는 종류: {}", it.id);
            }
            if let Kind::Link(_) = it.kind {
                assert!(!it.btn.is_empty(), "창 열기 단추 글이 없음: {}", it.id);
            }
        }
        for c in Cat::ALL {
            assert_eq!(Cat::from_key(c.key()), Some(c));
            assert!(ITEMS.iter().any(|i| i.cat == c), "빈 분류: {:?}", c);
        }
    }

    #[test]
    fn 기본값_복원은_비밀_주소_파일_언어를_건드리지_않는다() {
        for it in ITEMS.iter().filter(|i| i.reset) {
            let top = it.path[0];
            assert!(!["obs", "remote", "lanOutput", "ai", "triggerKey", "syncDir", "templates", "activeTemplates", "logoScreenImage", "uiLang", "port", "devices", "companion", "cameras"].contains(&top), "복원에서 빼야 하는 칸: {}", it.id);
        }
    }

    #[test]
    fn 이름_설명_관련_낱말로_찾는다() {
        let ids = |q: &str| search(q).iter().map(|i| i.id).collect::<Vec<_>>();
        assert!(ids("글자 크기").contains(&"ui.scale"));
        assert!(ids("확대").contains(&"ui.scale"), "관련 낱말");
        assert!(ids("모니터").contains(&"out-open"));
        assert!(ids("화살표").contains(&"output.cursorHidden"), "설명 속 낱말");
        assert!(ids("websocket").contains(&"obs-remote"));
        assert!(ids("QR").contains(&"remote"), "대소문자 무시");
        assert!(ids("백업").contains(&"sync"));
        assert!(ids("로고 그림").contains(&"logoScreenImage"), "낱말 둘 다 있어야 함");
        assert!(ids("없는낱말xyz").is_empty());
        assert!(ids("  ").is_empty());
    }

    #[test]
    fn 한_칸만_고치고_다른_칸은_그대로() {
        let dir = std::env::temp_dir().join(format!("wp-setitems-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let mut core = wp_core::core::Core::open(dir.clone(), dir.clone(), None).unwrap();
        core.settings.update(|d| {
            d["obs"]["passwordEnc"] = json!("비밀");
            d["앞으로_생길_칸"] = json!({ "a": [1, 2, 3] });
        });
        let core: Shared = std::sync::Arc::new(wp_core::lockcheck::Mutex::new(core));
        let before = core.lock().settings.get().clone();
        assert_eq!(patch(&core, &["output", "cursorHidden"], json!(false)), json!(true));
        let mut want = before.clone();
        want["output"]["cursorHidden"] = json!(false);
        assert_eq!(core.lock().settings.get(), &want, "고친 칸 말고는 모두 그대로 (모르는 칸·비밀 포함)");
        assert_eq!(patch(&core, &["output", "cursorHidden"], json!(false)), json!(false), "같은 값이면 아무것도 안 함");
        drop(core);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod small_song_settings_tests {
    use super::*;
    #[test]
    fn small_song_settings_registered_in_search_category() {
        for key in ["songSearchUrl", "songDownloadDir", "songInboxDir"] {
            let it = ITEMS.iter().find(|it| it.id == key).unwrap();
            assert_eq!(it.cat, Cat::Send);
            assert_eq!(it.path, &[key]);
            assert!(matches!(it.kind, Kind::Text));
            assert!(!it.reset);
        }
    }
}
