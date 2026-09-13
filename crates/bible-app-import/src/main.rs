use bible_app_import::import_from;
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "bible-app-import",
    about = "Import public-domain KJV text from a source dump into SQLite."
)]
struct Args {
    /// Directory that contains KJV.bt0 / KJV.bt3 / KJV.bt4 / KJV.bt7
    #[arg(long)]
    from: PathBuf,
    /// SQLite file to write (replaced if it exists)
    #[arg(long)]
    out: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match import_from(&args.from, &args.out) {
        Ok(stats) => {
            stats.report.print();
            eprintln!(
                "wrote {} KJV verses, {} comments, {} xrefs, {} word maps, {} Strong's entries, {} dictionary entries → {}",
                stats.verses,
                stats.resources,
                stats.xrefs,
                stats.verse_words,
                stats.strongs,
                stats.entries,
                args.out.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("bible-app-import: {e}");
            ExitCode::FAILURE
        }
    }
}
