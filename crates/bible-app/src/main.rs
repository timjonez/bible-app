mod app;
mod config;
mod dict;
mod history;
mod layout;
mod marks;
mod mhc;
mod nav;
mod occurrences;
mod passage;
mod picker;
mod search;
mod shell;
mod strongs;
mod tsk;
mod tsk_parse;
mod user_db;
mod workspace;

fn main() {
    let app = relm4::RelmApp::new("io.github.timjonez.bible-app");
    app.run::<app::App>(());
}
