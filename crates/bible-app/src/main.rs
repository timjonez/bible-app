fn main() {
    eprintln!(
        "bible-app GUI is not built yet (phase 1).\n\
         Import KJV first:\n\
         cargo run -p bible-app-import -- --from /path/to/PowerBibleCD --out data/bible-app.sqlite"
    );
    std::process::exit(1);
}
