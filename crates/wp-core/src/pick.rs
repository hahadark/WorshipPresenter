//! 라이브러리·찬송가·검색에서 고른 것을 큐시트에 넣기 · '바로 송출' 일 때 고른 것을 바로 내보내기
//!
//! 사용자 지적 (2026-11-14 리허설): 찬송가 탭에서 '3장 성부 성자와 성령' 을 골라 바로 띄우려 했는데
//! 넣기만 되고 화면에 안 나가서 여러 번 눌렀고, 누를 때마다(두 번 누르기는 두 번) 같은 순서가 하나씩 늘어 2초 동안 5개가 됐다.
//!  - Electron `cue.js addItem` 은 넣은 순서를 곧바로 `previewItem(at)` 하고, '미리보기 끔' 이면 `directSend` 로 송출한다.
//!    Rust 판 Act::AddItem 은 넣기만 하고 미리보기를 하지 않아 바로 송출 모드에서도 아무것도 안 나갔다.
//!  - 같은 찬송가(같은 파일·번호)·같은 라이브러리 찬양(songId)·같은 그림/영상/음악 파일을 다시 고르면
//!    새 순서를 만들지 않고 이미 있는 순서를 고른다 (바로 송출 모드면 송출). 알림 [그래도 하나 더 넣기] 로 일부러 더 넣을 수 있다.
//!  - 같은 것을 짧은 시간(RAPID) 안에 또 넣으려 하면(두 번 누르기·연타) 무시한다.
//!  성경·공지·제목·빈 화면 같은 새로 쓰는 순서는 같은 내용이 여러 번 나오는 것이 자연스러워(같은 본문을 두 번 읽기 등) 중복 검사에서 뺀다.
use serde_json::Value;
use std::time::{Duration, Instant};

/// 같은 것을 다시 넣으려 할 때 '연타' 로 보는 시간
pub const RAPID: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct PickState {
    /// 마지막으로 넣은 것 (content_key, 시각)
    pub(crate) last: Option<(String, Instant)>,
    /// [그래도 하나 더 넣기] 를 누르면 넣을 것 (순서, 넣을 자리)
    pub dup_pending: Option<(Value, Option<usize>)>,
    /// 만드는 중(찬송가 PPT)이라 아직 못 띄운 순서: (큐시트 id, 순서 번호). 다 만들면 미리보기(바로 송출이면 송출)
    pub want: Option<(String, usize)>,
    /// want 순서를 두 번 눌러 '송출' 하려던 것이면 그때의 송출 version — 다 만들었을 때 그대로면 이어서 송출
    pub want_send: Option<u64>,
}

/// 넣은 결과
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picked {
    /// 새 순서로 넣음 (자리)
    Added(usize),
    /// 이미 있는 순서를 고름 (자리)
    Existing(usize),
    /// 연타라서 무시
    Ignored,
}

fn norm_file(s: &str) -> String {
    s.trim().replace('/', "\\").to_lowercase()
}

/// 라이브러리에서 고른 것인지 알아보는 열쇠: 같은 열쇠면 같은 찬송가·찬양·파일
pub fn lib_key(it: &Value) -> Option<String> {
    let s = |k: &str| it[k].as_str().map(str::trim).filter(|x| !x.is_empty());
    match it["type"].as_str()? {
        "hymn" => s("file").map(|f| format!("hymn:{}", norm_file(f))).or_else(|| it["number"].as_i64().map(|n| format!("hymn#{n}"))),
        "song" => s("songId").map(|x| format!("song:{x}")).or_else(|| {
            // 베들레헴 찬송가처럼 songId 없이 가사로 들어오는 찬양: 제목 + 가사가 같으면 같은 곡
            let (t, l) = (s("title")?, s("lyrics")?);
            Some(format!("song~{t}\n{l}"))
        }),
        "image" => s("image").map(|x| format!("image:{}", norm_file(x))),
        "video" | "audio" => s("file").map(|x| format!("{}:{}", it["type"].as_str().unwrap_or(""), norm_file(x))),
        _ => None,
    }
}

/// 연타 검사용 열쇠: 라이브러리 열쇠, 없으면 id 를 뺀 내용 전체
pub fn content_key(it: &Value) -> String {
    if let Some(k) = lib_key(it) {
        return k;
    }
    let mut x = it.clone();
    if let Some(o) = x.as_object_mut() {
        o.remove("id");
    }
    x.to_string()
}

impl PickState {
    /// 같은 것을 RAPID 안에 또 넣으려 하면 true (시각은 늘 새로 적어 연타가 이어지는 동안 계속 막는다)
    pub fn rapid(&mut self, key: &str, now: Instant) -> bool {
        let hit = self.last.as_ref().is_some_and(|(k, t)| k == key && now.duration_since(*t) < RAPID);
        if hit {
            self.last = Some((key.to_string(), now));
        }
        hit
    }
    /// 연타 기억 지우기 (시험용 · [그래도 하나 더 넣기])
    pub fn forget_last(&mut self) {
        self.last = None;
    }
    pub fn mark(&mut self, key: String, now: Instant) {
        self.last = Some((key, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hymn_same_file_or_number() {
        let a = json!({ "type": "hymn", "file": "D:\\자료\\003 성부 성자와 성령.pps", "number": 3 });
        let b = json!({ "type": "hymn", "file": "d:/자료/003 성부 성자와 성령.PPS", "number": 3, "id": "x" });
        assert_eq!(lib_key(&a), lib_key(&b));
        assert_eq!(lib_key(&json!({ "type": "hymn", "file": "", "number": 3 })).as_deref(), Some("hymn#3"));
        assert_ne!(lib_key(&a), lib_key(&json!({ "type": "hymn", "file": "D:\\자료\\004.pps", "number": 4 })));
    }

    #[test]
    fn new_items_have_no_lib_key() {
        for t in ["bible", "text", "title", "blank", "camera"] {
            assert!(lib_key(&json!({ "type": t, "title": "같은 제목" })).is_none(), "{t}");
        }
        assert!(lib_key(&json!({ "type": "song", "title": "빈 찬양", "lyrics": "" })).is_none());
        assert!(lib_key(&json!({ "type": "image", "image": "" })).is_none());
    }

    #[test]
    fn rapid_repeat_is_blocked_and_extends() {
        let mut p = PickState::default();
        let t0 = Instant::now();
        assert!(!p.rapid("k", t0));
        p.mark("k".into(), t0);
        assert!(p.rapid("k", t0 + Duration::from_millis(1000)));
        assert!(p.rapid("k", t0 + Duration::from_millis(2000)), "연타가 이어지면 계속 막는다");
        assert!(!p.rapid("다른 것", t0 + Duration::from_millis(2100)));
        assert!(!p.rapid("k", t0 + Duration::from_millis(4000)));
    }

    #[test]
    fn content_key_ignores_id() {
        assert_eq!(content_key(&json!({ "id": "a", "type": "title", "title": "광고" })), content_key(&json!({ "id": "b", "type": "title", "title": "광고" })));
    }
}
