mod app;
mod config;
mod nav;

fn main() {
    let app = relm4::RelmApp::new("io.github.timjonez.bible-app");
    app.run::<app::App>(());
}
