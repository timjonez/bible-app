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
mod theme;
mod tsk;
mod tsk_parse;
mod user_db;
mod workspace;

fn main() {
    let id = std::env::var("BIBLE_APP_ID")
        .unwrap_or_else(|_| "io.github.timjonez.bible-app".to_string());
    let app = relm4::RelmApp::new(&id);
    app.run::<app::App>(());
}
