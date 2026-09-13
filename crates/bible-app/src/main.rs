mod app;
mod config;
mod mhc;
mod nav;
mod search;

fn main() {
    let app = relm4::RelmApp::new("io.github.timjonez.bible-app");
    app.run::<app::App>(());
}
