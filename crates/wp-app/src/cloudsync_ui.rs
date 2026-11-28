//! 클라우드 연결 창 안의 '자료 동기화' 구획 (웹판 2단계 B, wp_core::cloudsync). 기본 꺼짐.
//! cloud_ui.rs body 에서 한 줄로 부른다. 주소 · 열쇠 · 켜기는 클라우드 연결 것을 그대로 쓴다.

use eframe::egui::{self, RichText};
use wp_core::core::Shared;

fn t(ko: &str, en: &str) -> String {
    if wp_core::i18n::is_en() { en.to_string() } else { ko.to_string() }
}

pub fn section(ui: &mut egui::Ui, core: &Shared) {
    use crate::widgets::{form_row, group_head};
    let p = crate::theme::pal();
    group_head(ui, &t("자료 동기화", "Data sync"));
    let (mut on, cf) = {
        let c = core.lock();
        (wp_core::cloudsync::wanted(&c), wp_core::cloudsync::sync_conf(&c))
    };
    if form_row(ui, "", |ui| ui.checkbox(&mut on, t("큐시트 · 찬양 · 템플릿 · 테마 · 교회 공통 설정을 클라우드와 맞추기", "Sync cue sheets · songs · templates · themes · church settings with the cloud"))).changed() {
        wp_core::cloudsync::set_wanted(&mut core.lock(), on);
    }
    ui.add(
        egui::Label::new(
            RichText::new(t(
                "켜면 위 자료와 거기 쓰인 그림 · 영상이 교회 클라우드에 올라가고, 웹에서 고친 것이 이 PC 로 돌아옵니다. 양쪽에서 함께 고치면 덮어쓰지 않고 '충돌 사본' 을 남깁니다. PC 의 폴더 경로는 올리지 않습니다. 인터넷이 끊겨도 이 PC 는 그대로 돌고, 다시 이어지면 뒤에서 맞춥니다.",
                "When on, the items above and the images · videos they use go to your church cloud, and edits made on the web come back here. Edits made on both sides are never overwritten — a 'conflict copy' is kept. Folder paths on this PC are not uploaded. Without internet this PC works as before and catches up later.",
            ))
            .small()
            .color(p.muted),
        )
        .wrap(),
    );
    if !on {
        return;
    }
    let st = wp_core::cloudsync::status();
    ui.horizontal(|ui| {
        let (mark, text) = if !cf.on {
            ("info", t("클라우드 연결을 켜고 주소 · 연결 열쇠를 넣으면 시작합니다", "Starts once the cloud connection is on with an address and key"))
        } else {
            match st.state.as_str() {
                "ok" => ("ok", t("맞춤", "In sync")),
                "syncing" => ("info", if st.progress.is_empty() { t("맞추는 중…", "Syncing…") } else { format!("{} {}", t("올리는 중", "Uploading"), st.progress) }),
                "offline" => ("warn", t("끊김 — 뒤에서 다시", "Offline — retrying")),
                "error" => ("warn", t("멈춤", "Stopped")),
                _ => ("info", t("기다리는 중", "Waiting")),
            }
        };
        crate::widgets::status_mark(ui, mark);
        ui.label(text);
        if st.last_ok > 0 {
            ui.label(RichText::new(wp_core::i18n::tr_args(&t("올림 {1} · 받음 {2} · 충돌 사본 {3} · 파일 {4}", "sent {1} · received {2} · conflict copies {3} · files {4}"), &[&st.pushed, &st.pulled, &st.conflicts, &st.uploaded])).small().color(p.muted));
        }
    });
    if !st.error.is_empty() {
        ui.add(egui::Label::new(RichText::new(&st.error).small().color(p.danger_text)).wrap());
    }
    let note = match st.assets.as_str() {
        "missing" => t("이 클라우드에는 아직 그림·영상 저장소가 없어, 그림 주소는 가려서 올리고 출력은 이 PC 가 중계합니다.", "This cloud has no file store yet: image addresses are masked and outputs are relayed by this PC."),
        "quota" => t("클라우드 저장 한도에 가까워 그림·영상 올리기를 멈췄습니다 (출력은 이 PC 가 중계).", "File uploads paused near the cloud storage limit (outputs are relayed by this PC)."),
        "error" | "auth" => t("그림·영상을 올리지 못했습니다 — 30분 뒤 다시 해 봅니다.", "Could not upload files — will retry in 30 minutes."),
        _ => String::new(),
    };
    if !note.is_empty() {
        ui.label(RichText::new(note).small().color(p.muted));
    }
}
