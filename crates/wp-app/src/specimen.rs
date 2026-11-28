//! 자료·네트워크·출력 창 없이 여는 결정적 부품 견본. --specimen [icons] --shot <png>
use crate::{icons, theme::{self, Type, pal}, widgets::{self, Btn, Kind, Vis}};
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Vec2};

pub fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let value = |key: &str| args.iter().position(|s| s == key).and_then(|i| args.get(i + 1)).cloned();
    let settings: serde_json::Value = value("--data").and_then(|d| std::fs::read_to_string(std::path::Path::new(&d).join("settings.json")).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let dims = value("--win").and_then(|s| {let (w,h)=s.split_once('x')?; Some([w.parse().ok()?,h.parse().ok()?])}).unwrap_or([1440.0, 1000.0]);
    let only_icons = value("--specimen").as_deref() == Some("icons");
    let shot = value("--shot");
    eframe::run_native("WorshipPresenter · specimen", eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size(dims).with_resizable(false).with_position([0.0,0.0]), ..Default::default() }, Box::new(move |cc| {
        theme::install_fonts(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        crate::settings_ui::apply_ui_prefs(&cc.egui_ctx, &settings);
        Ok(Box::new(Specimen { shot, only_icons, frames: 0 }))
    })).map_err(|e| anyhow::anyhow!("{e}"))
}

struct Specimen { shot: Option<String>, only_icons: bool, frames: usize }
impl eframe::App for Specimen {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx=ui.ctx().clone();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal().panel).inner_margin(16)).show(ui, |ui| {
            ui.label(theme::ty("WorshipPresenter / CRAFT · 부품 견본",Type::Title,pal().text));
            ui.label(theme::ty("Pretendard · 4px grid · 90 / 130ms · 자료와 출력 서버를 열지 않는 점검 화면",Type::Small,pal().muted));
            ui.add_space(8.0);
            if self.only_icons { icon_grid(ui); } else { components(ui); }
        });
        self.frames+=1;
        if self.shot.is_some() && self.frames < 5 {ctx.request_repaint();}
        if self.frames == 4 {
            if let Some(path)=self.shot.clone() {
                let done=ctx.clone();
                ctx.request_screenshot(move |image| {
                    let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Some(parent)=std::path::Path::new(&path).parent() {let _=std::fs::create_dir_all(parent);}
                    let _=image::save_buffer(&path,&bytes,image.width() as u32,image.height() as u32,image::ColorType::Rgba8);
                    done.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            }
        }
    }
}

fn components(ui: &mut egui::Ui) {
    let p=pal();
    ui.horizontal(|ui| {
        for t in Type::ALL {
            ui.vertical(|ui| {ui.label(theme::ty(t.name(),t,p.text));ui.label(theme::ty(format!("{} / {}",t.size(),t.line()),Type::Micro,p.muted));});
            ui.add_space(8.0);
        }
    });
    ui.horizontal(|ui| {
        for (name,c) in [("바탕",p.bg),("판",p.panel),("머리",p.head),("조절 칸",p.raised),("입력",p.sunken),("글",p.text),("보조",p.text2),("흐린 글",p.muted),("선",p.line),("송출",p.live),("다음",p.next),("오류",p.danger_text),("초점",p.focus)] {
            ui.vertical(|ui| {let (r,_)=ui.allocate_exact_size(Vec2::new(48.0,16.0),Sense::hover());ui.painter().rect_filled(r,0.0,c);ui.label(theme::ty(name,Type::Micro,p.muted));});
        }
    });
    ui.add_space(8.0);
    let col_w=((ui.available_width()-24.0)/2.0).max(400.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(Vec2::new(col_w,0.0),egui::Layout::top_down(egui::Align::LEFT),|ui| {
            state_table(ui,col_w);
        });
        ui.add_space(16.0);
        ui.allocate_ui_with_layout(Vec2::new(col_w,0.0),egui::Layout::top_down(egui::Align::LEFT),|ui| {
            ui.label(theme::ty("표 / 목록 · 의미와 상태",Type::BodyStrong,p.text));
            for (title,v) in [("보통",widgets::RowVis::default()),("올림",widgets::RowVis{hover:1.0,..Default::default()}),("고름",widgets::RowVis{selected:true,..Default::default()}),("송출 중",widgets::RowVis{live:true,..Default::default()}),("다음",widgets::RowVis{next:true,..Default::default()}),("지나감",widgets::RowVis{done:true,..Default::default()}),("키보드 초점",widgets::RowVis{focus:true,..Default::default()})] {
                let (r,_)=ui.allocate_exact_size(Vec2::new(col_w,theme::ROW_H),Sense::hover());
                let ink=widgets::paint_row(ui,r,v);
                icons::paint(ui,Rect::from_min_size(r.min+Vec2::new(12.0,6.0),Vec2::splat(16.0)),"book-open",ink.icon);
                ui.painter().text(r.min+Vec2::new(40.0,14.0),Align2::LEFT_CENTER,title,Type::Body.font(),ink.text);
                widgets::tnum(ui,Pos2::new(r.right()-12.0,r.center().y),Align2::RIGHT_CENTER,"11:09",Type::Small.font(),ink.sub);
            }
            ui.add_space(8.0);
            for tone in [widgets::Tone::Info,widgets::Tone::Ok,widgets::Tone::Warn,widgets::Tone::Error] {widgets::notice(ui,tone,"알림 전문과 다음 할 일을 한 줄로 표시합니다",Some("확인"),true);}
            ui.add_space(8.0);
            widgets::empty(ui,"folder-open","아직 순서가 없습니다","순서를 넣거나 기존 큐시트를 열어 예배를 준비하세요.",Some("순서 넣기"));
            let (r,_)=ui.allocate_exact_size(Vec2::new(col_w,4.0),Sense::hover()); widgets::paint_progress(ui,r,Some(0.42),0.0);
            let (r,_)=ui.allocate_exact_size(Vec2::new(col_w,4.0),Sense::hover()); widgets::paint_progress(ui,r,None,0.4);
            ui.label(theme::ty("진행 42% / 기다림 (견본에서는 시각 고정)",Type::Small,p.muted));
            let mut tab=0;widgets::tabs(ui,&mut tab,&[(0,"글자"),(1,"무대"),(2,"미디어"),(3,"소스")]);
            let mut seg=0;widgets::segmented(ui,&mut seg,&[(0,"목록"),(1,"격자"),(2,"전체 목록")]);
            let(r,_)=ui.allocate_exact_size(Vec2::new(col_w,theme::SECTION_H),Sense::hover());widgets::paint_section_head(ui,r,"접는 구획",1.0,0.0,false);
            widgets::tip(ui.add(Btn::new("단축키 툴팁").kbd("F9")),"송출","F9");
            ui.horizontal(|ui| {ui.label("메뉴");widgets::kbd(ui,"Ctrl+K");ui.separator();ui.label("찾기");});
        });
    });
}

fn state_table(ui: &mut egui::Ui,w:f32) {
    let p=pal(); let label=88.0;let cw=(w-label)/6.0;
    let(r,_)=ui.allocate_exact_size(Vec2::new(w,24.0),Sense::hover());
    for(i,(name,_))in Vis::ALL.iter().enumerate(){ui.painter().text(Pos2::new(r.left()+label+cw*(i as f32+0.5),r.center().y),Align2::CENTER_CENTER,name,Type::Micro.font(),p.muted);}
    for title in ["주된 단추","보통 단추","조용한 단추","위험 (선)","아이콘","팝업","체크","라디오","스위치","슬라이더","글 입력칸"] {
        let(r,_)=ui.allocate_exact_size(Vec2::new(w,36.0),Sense::hover());
        ui.painter().text(Pos2::new(r.left(),r.center().y),Align2::LEFT_CENTER,title,Type::Small.font(),p.text2);
        for(i,(_,vis))in Vis::ALL.iter().enumerate(){
            let cell=Rect::from_min_size(Pos2::new(r.left()+label+cw*i as f32+4.0,r.top()+5.0),Vec2::new(cw-8.0,26.0));
            match title {
                "체크"=>widgets::paint_check(ui,Rect::from_center_size(cell.center(),Vec2::splat(16.0)),vis.on,*vis),
                "라디오"=>widgets::paint_radio(ui,Rect::from_center_size(cell.center(),Vec2::splat(16.0)),vis.on,*vis),
                "스위치"=>widgets::paint_switch(ui,Rect::from_center_size(cell.center(),Vec2::new(30.0,18.0)),if vis.on{1.0}else{0.0},*vis),
                "슬라이더"=>widgets::paint_slider(ui,cell,0.55,*vis),
                "글 입력칸"=>{widgets::paint_field(ui,cell,*vis);ui.painter().text(cell.center(),Align2::CENTER_CENTER,"본문",Type::Small.font(),if vis.off{p.muted2}else{p.text});},
                "팝업"=>widgets::paint_popup(ui,cell,"고르기",*vis,vis.on),
                _=>{let base=match title{"주된 단추"=>theme::mix(p.raised,74.0,egui::Color32::WHITE),"조용한 단추"|"위험 (선)"=>egui::Color32::TRANSPARENT,_=>if vis.on{p.sel}else{p.raised}};widgets::paint_ctl(ui,cell,base,*vis,title!="조용한 단추");if title=="아이콘"{icons::paint(ui,Rect::from_center_size(cell.center(),Vec2::splat(16.0)),"settings",if vis.off{p.muted2}else{p.text});}else{ui.painter().text(cell.center(),Align2::CENTER_CENTER,"단추",Type::Small.font(),if vis.off{p.muted2}else{p.text});}},
            }
        }
    }
    ui.add_space(8.0);
    ui.label(theme::ty("실제로 조작할 수 있는 부품",Type::BodyStrong,p.text));
    ui.horizontal(|ui| {for(k,t)in [(Kind::Primary,"주된 일"),(Kind::Default,"보통"),(Kind::Ghost,"조용한"),(Kind::Danger,"지우기")] {ui.add(Btn::new(t).kind(k));}});
    let mut check=false;widgets::checkbox(ui,&mut check,"체크 · Tab 다음 Space 로 바꾸기");
    let mut on=false;widgets::switch(ui,&mut on,"스위치");
    let mut text=ui.data(|d|d.get_temp::<String>(egui::Id::new("spec-text"))).unwrap_or_default();widgets::text_field(ui,&mut text,"찾기 (Ctrl+K)",w,Some("search"));ui.data_mut(|d|d.insert_temp(egui::Id::new("spec-text"),text));
}

fn icon_grid(ui:&mut egui::Ui){
    let p=pal();let names=icons::names();let cols=(ui.available_width()/104.0).floor().max(1.0) as usize;
    for row in names.chunks(cols){ui.horizontal(|ui|{for name in row {ui.allocate_ui(Vec2::new(100.0,48.0),|ui|{ui.horizontal(|ui|{for s in [16.0,20.0,24.0]{icons::show(ui,name,s,p.text);}});ui.label(theme::ty(*name,Type::Micro,p.muted));});}});}
}
