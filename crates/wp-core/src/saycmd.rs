//! 말로 시키기 — "지난주 큐시트 복사해서 설교만 바꿔" 같은 말을 정해진 명령(단계) 목록으로 바꿔, 보여 주고 확인받은 뒤 실행
//!
//! - AI 는 계획(JSON 단계)만 만든다. 실행은 여기서 정한 단계만 하고, 단계마다 인자를 검사한다.
//! - 송출 · 지우기(삭제) · 예배당 화면 · 설정 바꾸기는 단계에 없다 → AI 가 만들어도 '할 수 없는 일' 로 빠진다.
//!   장절은 '다음(미리보기)' 칸까지만 (preview_passage_quiet). 순서 바꾸기는 되돌리기(Ctrl+Z) 묶음 하나로.
//! - 키가 없으면 규칙(자주 쓰는 말 몇 가지)으로만.

use crate::aihelp::{self, AiCfg};
use crate::core::{new_item, Core};
use serde_json::{json, Value};

/// 할 수 있는 단계 (이름, 설명)
pub const ACTIONS: [(&str, &str); 9] = [
    ("open_cue", "큐시트 열기 {cue}"),
    ("duplicate_cue", "큐시트 복제 {cue} → 새 이름 {name} · 날짜 {date} (복제한 것을 연다)"),
    ("rename_cue", "열린 큐시트 이름·날짜 바꾸기 {name} {date}"),
    ("set_item", "열린 큐시트의 순서 하나 고치기 {match} {fields: title·subtitle·ref·body·lyrics·note}"),
    ("add_item", "순서 넣기 {type: title|bible|text|song|blank} {fields} {after}"),
    ("move_item", "순서 옮기기 {match} {to: 번호}"),
    ("preview_ref", "성경 장절을 '다음' 칸에 올리기 {ref} (송출은 하지 않음)"),
    ("open_tool", "창 열기 {tool: check|sermon|lyrics|recap|listen|points|bulletin|history}"),
    ("search", "찾기칸에서 찾기 {q}"),
];

/// 하지 않는 일 (AI 가 내면 빼고 알린다)
pub const FORBIDDEN_HINT: &str = "송출 · 화면 지우기 · 삭제 · 예배당 화면 켜고 끄기 · 설정 바꾸기는 말로 시킬 수 없습니다 (직접 하세요).";

const FIELDS: [&str; 7] = ["title", "subtitle", "ref", "body", "lyrics", "note", "more"];
const TOOLS: [&str; 8] = ["check", "sermon", "lyrics", "recap", "listen", "points", "bulletin", "history"];
const ITEM_TYPES: [&str; 5] = ["title", "bible", "text", "song", "blank"];

/// 검사한 단계
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub action: String,
    pub args: Value,
    /// 사람이 읽는 설명
    pub label: String,
}

/// 계획 검사 결과: (할 단계, 뺀 단계와 까닭)
pub fn validate(plan: &Value, ctx: &Ctx) -> (Vec<Step>, Vec<String>) {
    let mut ok = vec![];
    let mut bad = vec![];
    for st in plan["steps"].as_array().into_iter().flatten().take(12) {
        let a = st["action"].as_str().unwrap_or("").trim();
        let args = &st["args"];
        match check(a, args, ctx) {
            Ok(label) => ok.push(Step { action: a.to_string(), args: args.clone(), label }),
            Err(e) => bad.push(format!("{a}: {e}")),
        }
    }
    (ok, bad)
}

/// 계획을 만들 때 쓰는 지금 상태 (큐시트 목록 · 열린 큐시트 순서)
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    /// (id, 이름, 날짜)
    pub cues: Vec<(String, String, String)>,
    pub open_id: String,
    /// 열린 큐시트 순서 (type, title, 부가)
    pub items: Vec<(String, String, String)>,
    pub today: String,
}

impl Ctx {
    pub fn find_cue(&self, q: &str) -> Option<&(String, String, String)> {
        let q = q.trim();
        if q.is_empty() {
            return None;
        }
        self.cues.iter().find(|c| c.0 == q).or_else(|| self.cues.iter().find(|c| c.1 == q)).or_else(|| self.cues.iter().find(|c| c.1.contains(q) || (!c.2.is_empty() && c.2 == q)))
    }
    /// 순서 찾기: 번호(1부터) 또는 제목 일부 또는 종류
    pub fn find_item(&self, m: &Value) -> Option<usize> {
        if let Some(n) = m.as_i64().or_else(|| m["index"].as_i64()) {
            return (n >= 1 && (n as usize) <= self.items.len()).then(|| n as usize - 1);
        }
        let t = m.as_str().or(m["title"].as_str()).unwrap_or("").trim();
        if !t.is_empty() {
            if let Some(i) = self.items.iter().position(|x| x.1 == t).or_else(|| self.items.iter().position(|x| x.1.contains(t))) {
                return Some(i);
            }
        }
        let ty = m["type"].as_str().unwrap_or("");
        if !ty.is_empty() {
            return self.items.iter().position(|x| x.0 == ty);
        }
        None
    }
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("").trim()
}

fn check(a: &str, args: &Value, ctx: &Ctx) -> Result<String, String> {
    match a {
        "open_cue" => ctx.find_cue(s(args, "cue")).map(|c| format!("큐시트 열기: {}", c.1)).ok_or_else(|| format!("'{}' 큐시트를 찾지 못했습니다", s(args, "cue"))),
        "duplicate_cue" => {
            let c = ctx.find_cue(s(args, "cue")).ok_or_else(|| format!("'{}' 큐시트를 찾지 못했습니다", s(args, "cue")))?;
            let date = s(args, "date");
            if !date.is_empty() && crate::cuemgr::day_of(date).is_empty() {
                return Err(format!("날짜 '{date}' 를 읽을 수 없습니다"));
            }
            Ok(format!("큐시트 복제: {} → {}{} (복제본을 엽니다)", c.1, if s(args, "name").is_empty() { "같은 이름 + 복사본".to_string() } else { s(args, "name").to_string() }, if date.is_empty() { String::new() } else { format!(" · {date}") }))
        }
        "rename_cue" => {
            if s(args, "name").is_empty() && s(args, "date").is_empty() {
                return Err("바꿀 이름·날짜가 없습니다".into());
            }
            Ok(format!("큐시트 이름·날짜: {} {}", s(args, "name"), s(args, "date")).trim().to_string())
        }
        "set_item" => {
            let f = &args["fields"];
            let keys: Vec<&String> = f.as_object().map(|o| o.keys().collect()).unwrap_or_default();
            if keys.is_empty() || keys.iter().any(|k| !FIELDS.contains(&k.as_str())) {
                return Err(format!("고칠 칸은 {} 만 됩니다", FIELDS.join("·")));
            }
            if let Some(r) = f["ref"].as_str() {
                if let crate::parser::Parsed::Err { error, .. } = crate::parser::parse_reference(r) {
                    return Err(format!("장절 '{r}': {error}"));
                }
            }
            let what = f.as_object().unwrap().iter().map(|(k, v)| format!("{k}='{}'", v.as_str().unwrap_or("").chars().take(30).collect::<String>())).collect::<Vec<_>>().join(", ");
            // 복제하기 전이면 순서는 실행할 때 찾는다 (이름이 같으므로)
            match ctx.find_item(&args["match"]) {
                Some(i) => Ok(format!("{}번 '{}' 고치기: {what}", i + 1, ctx.items[i].1)),
                None if !ctx.items.is_empty() || args["match"].is_string() || args["match"].is_object() => Ok(format!("'{}' 순서 고치기: {what}", args["match"].as_str().map(String::from).unwrap_or_else(|| args["match"].to_string()))),
                None => Err("고칠 순서를 찾지 못했습니다".into()),
            }
        }
        "add_item" => {
            let ty = s(args, "type");
            if !ITEM_TYPES.contains(&ty) {
                return Err(format!("넣을 수 있는 종류는 {} 입니다", ITEM_TYPES.join("·")));
            }
            let f = &args["fields"];
            if f.as_object().is_some_and(|o| o.keys().any(|k| !FIELDS.contains(&k.as_str()))) {
                return Err(format!("칸은 {} 만 됩니다", FIELDS.join("·")));
            }
            if ty == "bible" {
                let r = f["ref"].as_str().unwrap_or("");
                if let crate::parser::Parsed::Err { error, .. } = crate::parser::parse_reference(r) {
                    return Err(format!("장절 '{r}': {error}"));
                }
            }
            Ok(format!("순서 넣기: {} '{}'{}", ty, f["title"].as_str().or(f["ref"].as_str()).unwrap_or(""), if args["after"].is_null() { " (끝에)".to_string() } else { format!(" ({} 뒤에)", args["after"].as_str().map(String::from).unwrap_or_else(|| args["after"].to_string())) }))
        }
        "move_item" => {
            let i = ctx.find_item(&args["match"]).ok_or("옮길 순서를 찾지 못했습니다")?;
            let to = args["to"].as_i64().ok_or("옮길 자리(번호)가 없습니다")?;
            if to < 1 || to as usize > ctx.items.len() {
                return Err("자리 번호가 범위를 벗어났습니다".into());
            }
            Ok(format!("{}번 '{}' 을 {to}번 자리로", i + 1, ctx.items[i].1))
        }
        "preview_ref" => match crate::parser::parse_reference(s(args, "ref")) {
            crate::parser::Parsed::Ok { .. } => Ok(format!("'다음' 칸에 올리기: {} (송출은 직접)", s(args, "ref"))),
            crate::parser::Parsed::Err { error, .. } => Err(error),
        },
        "open_tool" => {
            let t = s(args, "tool");
            if TOOLS.contains(&t) { Ok(format!("창 열기: {t}")) } else { Err(format!("모르는 창 '{t}'")) }
        }
        "search" => {
            if s(args, "q").is_empty() { Err("찾을 말이 없습니다".into()) } else { Ok(format!("찾기: {}", s(args, "q"))) }
        }
        _ => Err("할 수 없는 일입니다".into()),
    }
}

// ---------------- 규칙 (AI 없이) ----------------
/// 자주 쓰는 말 몇 가지: 장절 보여 줘 · 지난주 큐시트 복사 · 점검 · 가사 정리 · 예배 후 정리 · 설교 원고
pub fn rule_plan(text: &str, ctx: &Ctx) -> Option<Value> {
    let t = text.trim();
    let mut steps: Vec<Value> = vec![];
    if t.contains("지난주") || t.contains("지난 주") || t.contains("저번 주") {
        // 오늘보다 앞선 가장 최근 날짜의 큐시트 (열린 것 말고)
        let mut past: Vec<&(String, String, String)> = ctx.cues.iter().filter(|c| !c.2.is_empty() && c.2 < ctx.today).collect();
        past.sort_by(|a, b| b.2.cmp(&a.2));
        if let Some(c) = past.first() {
            if t.contains("복사") || t.contains("복제") || t.contains("가져") {
                let date = crate::cuemgr::add_days(&c.2, 7);
                let name = if c.1.contains(&c.2) { c.1.replacen(&c.2, &date, 1) } else { format!("{} ({date})", c.1) };
                steps.push(json!({ "action": "duplicate_cue", "args": { "cue": c.0, "name": name, "date": date } }));
            } else {
                steps.push(json!({ "action": "open_cue", "args": { "cue": c.0 } }));
            }
        }
    }
    if let Some(f) = crate::refscan::find_refs(t).first() {
        if t.contains("보여") || t.contains("띄워") || t.contains("올려") || t.contains("다음") || steps.is_empty() {
            steps.push(json!({ "action": "preview_ref", "args": { "ref": f.reference } }));
        }
    }
    for (kw, tool) in [("점검", "check"), ("가사", "lyrics"), ("예배 후", "recap"), ("유튜브", "recap"), ("원고", "sermon"), ("받아 적", "listen"), ("자막", "listen")] {
        if t.contains(kw) && steps.is_empty() {
            steps.push(json!({ "action": "open_tool", "args": { "tool": tool } }));
        }
    }
    if steps.is_empty() { None } else { Some(json!({ "steps": steps, "say": "" })) }
}

// ---------------- AI ----------------
pub fn ai_system() -> String {
    let acts = ACTIONS.iter().map(|(a, d)| format!("- {a}: {d}")).collect::<Vec<_>>().join("\n");
    format!(
        "너는 교회 예배 송출 프로그램 '워십프리젠터' 의 조작 도우미다. 사용자의 말을 아래 단계 목록으로만 바꾼다.\n{acts}\n\
         규칙:\n- 송출 · 화면 지우기 · 삭제 · 예배당 화면 · 설정은 단계가 없으니 만들지 않는다 (그런 요청이면 steps 를 비우고 say 에 '직접 하세요').\n\
         - cue 는 주어진 큐시트 id 를 쓴다. '지난주' 는 오늘 날짜보다 앞선 가장 최근 예배 날짜의 큐시트.\n\
         - 복제할 때 date 는 원본 날짜 + 7일(다음 예배), name 은 원본 이름 속 날짜를 그 날짜로 바꾼 것.\n\
         - duplicate_cue 다음의 set_item·add_item 은 복제한 큐시트(열린 것)에 적용된다. match 는 순서 제목 일부(예: \"설교\") 또는 번호.\n\
         - 설교 제목은 보통 '설교' 제목 순서의 subtitle (\"“제목” · 설교자\" 꼴), 본문은 bible 순서의 ref.\n\
         - 장절은 '롬 8:28' 꼴. 모르는 정보는 지어내지 말고 say 에 물어본다.\n\
         JSON 하나만: {{\"steps\":[{{\"action\":\"…\",\"args\":{{…}}}}],\"say\":\"사용자에게 한 줄\"}}"
    )
}

pub fn ai_user(text: &str, ctx: &Ctx) -> String {
    let cues = ctx.cues.iter().take(20).map(|c| format!("- id={} · {} · 날짜 {}", c.0, c.1, if c.2.is_empty() { "-" } else { &c.2 })).collect::<Vec<_>>().join("\n");
    let items = ctx.items.iter().enumerate().map(|(i, x)| format!("{}. [{}] {} {}", i + 1, x.0, x.1, x.2)).collect::<Vec<_>>().join("\n");
    format!("오늘: {}\n큐시트 목록:\n{cues}\n열린 큐시트(id={}) 순서:\n{items}\n\n사용자: {text}", ctx.today, ctx.open_id)
}

pub fn ask_ai(cfg: &AiCfg, text: &str, ctx: &Ctx) -> Result<(Value, Value), String> {
    aihelp::chat_json(cfg, &ai_system(), &ai_user(text, ctx), 1200)
}

// ---------------- 실행 ----------------
impl Core {
    pub fn say_ctx(&self) -> Ctx {
        let cues = self.cues.iter().filter(|c| !c.template).map(|c| (c.id.clone(), c.name.clone(), c.service_date.clone())).collect();
        let items = self
            .cue
            .as_ref()
            .map(|c| {
                c.items
                    .iter()
                    .map(|it| {
                        let extra = ["ref", "subtitle"].iter().filter_map(|k| it[*k].as_str().filter(|x| !x.is_empty())).next().unwrap_or("").to_string();
                        (it["type"].as_str().unwrap_or("").to_string(), it["title"].as_str().unwrap_or("").to_string(), extra)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ctx { cues, open_id: self.cue.as_ref().map(|c| c.id.clone()).unwrap_or_default(), items, today: crate::cuemgr::today() }
    }

    /// 확인받은 단계를 차례로 실행. 반환: 단계마다 결과 한 줄 · 화면이 할 일(open_tool · search)
    pub fn run_say_steps(&mut self, steps: &[Step]) -> (Vec<String>, Vec<(String, String)>) {
        let mut log = vec![];
        let mut ui: Vec<(String, String)> = vec![];
        let mut remembered = false;
        for st in steps {
            let ctx = self.say_ctx();
            // 다시 검사 (앞 단계가 큐시트를 바꿨을 수 있다)
            if let Err(e) = check(&st.action, &st.args, &ctx) {
                log.push(format!("건너뜀 — {}: {e}", st.label));
                continue;
            }
            let a = &st.args;
            let r: Result<String, String> = match st.action.as_str() {
                "open_cue" => {
                    let id = ctx.find_cue(s(a, "cue")).map(|c| c.0.clone()).unwrap_or_default();
                    self.open_cue(&id);
                    remembered = false;
                    Ok(st.label.clone())
                }
                "duplicate_cue" => {
                    let id = ctx.find_cue(s(a, "cue")).map(|c| c.0.clone()).unwrap_or_default();
                    let name = Some(s(a, "name")).filter(|x| !x.is_empty());
                    let date = Some(s(a, "date")).filter(|x| !x.is_empty());
                    match self.duplicate_cue_as(&id, name, date, false) {
                        Ok(c) => {
                            self.open_cue(&c.id);
                            remembered = false;
                            Ok(format!("복제해서 열었습니다: {}", c.name))
                        }
                        Err(e) => Err(e.to_string()),
                    }
                }
                "rename_cue" => {
                    let id = ctx.open_id.clone();
                    let mut m = json!({});
                    if !s(a, "name").is_empty() {
                        m["name"] = json!(s(a, "name"));
                    }
                    if !s(a, "date").is_empty() {
                        m["serviceDate"] = json!(s(a, "date"));
                    }
                    self.update_cue_meta_of(&id, &m).map(|_| st.label.clone()).map_err(|e| e.to_string())
                }
                "set_item" | "add_item" | "move_item" => {
                    if !remembered {
                        self.remember_items(); // 순서 바꾸기는 되돌리기 한 번으로
                        remembered = true;
                    }
                    self.say_item_step(&st.action, a, &ctx).map(|_| st.label.clone())
                }
                "preview_ref" => self.preview_passage_quiet(s(a, "ref")).map(|t| format!("'다음' 칸에 올림: {t} (송출은 직접)")),
                "open_tool" => {
                    ui.push(("tool".into(), s(a, "tool").to_string()));
                    Ok(st.label.clone())
                }
                "search" => {
                    ui.push(("search".into(), s(a, "q").to_string()));
                    Ok(st.label.clone())
                }
                _ => Err("할 수 없는 일".into()),
            };
            log.push(match r {
                Ok(m) => format!("했음 — {m}"),
                Err(e) => format!("못 함 — {}: {e}", st.label),
            });
        }
        if remembered {
            self.save_cue();
        }
        self.emit(crate::core::Ev::Ui);
        (log, ui)
    }

    fn say_item_step(&mut self, action: &str, a: &Value, ctx: &Ctx) -> Result<(), String> {
        let cue = self.cue.as_mut().ok_or("열린 큐시트가 없습니다")?;
        match action {
            "set_item" => {
                let i = ctx.find_item(&a["match"]).ok_or("고칠 순서를 찾지 못했습니다")?;
                let it = cue.items.get_mut(i).ok_or("순서가 없습니다")?;
                for (k, v) in a["fields"].as_object().into_iter().flatten() {
                    if FIELDS.contains(&k.as_str()) {
                        it[k] = json!(v.as_str().unwrap_or(""));
                    }
                }
                Ok(())
            }
            "add_item" => {
                let ty = s(a, "type");
                let f = &a["fields"];
                let mut it = new_item(ty, f["title"].as_str().unwrap_or(match ty { "bible" => "성경", "text" => "공지", "song" => "찬양", "blank" => "빈 화면", _ => "제목" }));
                for (k, v) in f.as_object().into_iter().flatten() {
                    if FIELDS.contains(&k.as_str()) {
                        it[k] = json!(v.as_str().unwrap_or(""));
                    }
                }
                let at = if a["after"].is_null() { cue.items.len() } else { ctx.find_item(&a["after"]).map(|i| i + 1).unwrap_or(cue.items.len()) };
                cue.items.insert(at.min(cue.items.len()), it);
                Ok(())
            }
            "move_item" => {
                let i = ctx.find_item(&a["match"]).ok_or("옮길 순서를 찾지 못했습니다")?;
                let to = (a["to"].as_i64().unwrap_or(1).max(1) as usize - 1).min(cue.items.len() - 1);
                let it = cue.items.remove(i);
                cue.items.insert(to, it);
                Ok(())
            }
            _ => Err("할 수 없는 일".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ctx() -> Ctx {
        Ctx {
            cues: vec![("c1".into(), "2026-11-22 주일 예배".into(), "2026-11-22".into()), ("c2".into(), "2026-11-15 주일 예배".into(), "2026-11-15".into())],
            open_id: "c1".into(),
            items: vec![("title".into(), "예배로 부름".into(), String::new()), ("bible".into(), "성경봉독".into(), "요 15:1-8".into()), ("title".into(), "설교".into(), "“포도나무” · 홍길동".into())],
            today: "2026-11-27".into(),
        }
    }
    #[test]
    fn dangerous_actions_are_dropped() {
        let plan = json!({"steps":[
            {"action":"send_live","args":{}},
            {"action":"delete_item","args":{"match":"설교"}},
            {"action":"clear_all","args":{}},
            {"action":"set_item","args":{"match":"설교","fields":{"subtitle":"“은혜” · 홍길동","tplMain":"x"}}},
            {"action":"preview_ref","args":{"ref":"롬 8:28"}},
            {"action":"open_tool","args":{"tool":"settings"}}
        ]});
        let (ok, bad) = validate(&plan, &ctx());
        assert_eq!(ok.len(), 1, "{ok:?}");
        assert_eq!(ok[0].action, "preview_ref");
        assert_eq!(bad.len(), 5, "{bad:?}");
        assert!(bad[0].contains("할 수 없는"));
    }
    #[test]
    fn duplicate_then_edit_plan() {
        let plan = json!({"steps":[
            {"action":"duplicate_cue","args":{"cue":"c1","name":"2026-11-29 주일 예배","date":"2026-11-29"}},
            {"action":"set_item","args":{"match":"설교","fields":{"subtitle":"“은혜” · 홍길동"}}},
            {"action":"set_item","args":{"match":{"type":"bible"},"fields":{"ref":"롬 8:28"}}},
            {"action":"add_item","args":{"type":"bible","fields":{"title":"함께 읽기","ref":"엉터리"}}}
        ]});
        let (ok, bad) = validate(&plan, &ctx());
        assert_eq!(ok.len(), 3);
        assert!(ok[0].label.contains("2026-11-22 주일 예배 → 2026-11-29 주일 예배"), "{}", ok[0].label);
        assert!(ok[1].label.starts_with("3번 '설교' 고치기"));
        assert!(ok[2].label.starts_with("2번 '성경봉독' 고치기"));
        assert_eq!(bad.len(), 1);
    }
    #[test]
    fn rules_without_ai() {
        let p = rule_plan("지난주 큐시트 복사해줘", &ctx()).unwrap();
        assert_eq!(p["steps"][0]["action"], "duplicate_cue");
        assert_eq!(p["steps"][0]["args"]["cue"], "c1");
        assert_eq!(p["steps"][0]["args"]["date"], "2026-11-29");
        assert_eq!(p["steps"][0]["args"]["name"], "2026-11-29 주일 예배");
        let p = rule_plan("요한복음 3장 16절 보여줘", &ctx()).unwrap();
        assert_eq!(p["steps"][0]["action"], "preview_ref");
        assert_eq!(p["steps"][0]["args"]["ref"], "요한복음 3:16");
        assert_eq!(rule_plan("예배 전 점검", &ctx()).unwrap()["steps"][0]["args"]["tool"], "check");
        assert!(rule_plan("송출해", &ctx()).is_none());
    }
    #[test]
    fn run_on_core() {
        let dir = std::env::temp_dir().join(format!("wp-say-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir, None).unwrap();
        let cue = crate::db::Cue { id: "c1".into(), name: "2026-11-22 주일 예배".into(), service_date: "2026-11-22".into(), start_time: String::new(), items: vec![json!({"id":"a","type":"title","title":"설교","subtitle":"옛"}), json!({"id":"b","type":"bible","title":"성경봉독","ref":"요 1:1"})], memo: String::new(), pinned: false, template: false };
        c.store.save_cue(&cue).unwrap();
        c.cues = c.store.list_cues();
        c.open_cue("c1");
        let plan = json!({"steps":[
            {"action":"duplicate_cue","args":{"cue":"c1","name":"2026-11-29 주일 예배","date":"2026-11-29"}},
            {"action":"set_item","args":{"match":"설교","fields":{"subtitle":"“은혜” · 홍길동"}}},
            {"action":"add_item","args":{"type":"text","fields":{"title":"광고","body":"수요예배"},"after":"설교"}}
        ]});
        let (ok, _) = validate(&plan, &c.say_ctx());
        let (log, _) = c.run_say_steps(&ok);
        assert!(log.iter().all(|l| l.starts_with("했음")), "{log:?}");
        let open = c.cue.clone().unwrap();
        assert_ne!(open.id, "c1");
        assert_eq!(open.name, "2026-11-29 주일 예배");
        assert_eq!(open.items[0]["subtitle"], "“은혜” · 홍길동");
        assert_eq!(open.items[1]["title"], "광고");
        assert_eq!(c.store.get_cue("c1").unwrap().items[0]["subtitle"], "옛", "원본은 그대로");
        // 되돌리기 한 번에 순서 바꾼 것이 모두 돌아간다
        assert!(c.undo_items(false));
        assert_eq!(c.cue.as_ref().unwrap().items.len(), 2);
        assert!(c.pres.live.is_none(), "송출은 하지 않음");
    }
}
