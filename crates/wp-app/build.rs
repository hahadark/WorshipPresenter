include!("../winres.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../winres.rs");
    let icon = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("..").join("..").join("assets").join("icon.ico");
    embed("워십프리젠터", "WorshipPresenter.exe", &icon);
}
