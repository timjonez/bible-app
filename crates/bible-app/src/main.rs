mod app;
mod config;
mod dict;
mod history;
mod layout;
mod mhc;
mod nav;
mod search;
mod sidebar;
mod strongs;
mod tsk;
mod tsk_parse;

fn main() {
    let app = relm4::RelmApp::new("io.github.timjonez.bible-app");
    app.run::<app::App>(());
}
