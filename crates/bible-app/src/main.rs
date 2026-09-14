mod app;
mod config;
mod dict;
mod history;
mod layout;
mod mhc;
mod nav;
mod search;
mod strongs;
mod tsk;

fn main() {
    let app = relm4::RelmApp::new("io.github.timjonez.bible-app");
    app.run::<app::App>(());
}
