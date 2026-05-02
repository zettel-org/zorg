use std::env;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("-h" | "--help") => print_help(),
        Some("-V" | "--version") => println!("zorg {VERSION}"),
        Some("check" | "fix" | "query" | "capture" | "index") => {
            eprintln!("zorg command behavior is pending; this is an Epic 1 workspace stub");
            std::process::exit(2);
        }
        Some(command) => {
            eprintln!("unknown zorg command: {command}");
            eprintln!("run `zorg --help` for usage");
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!(
        "\
zorg {VERSION}

Usage: zorg [OPTIONS] [COMMAND]

Commands:
  index     Placeholder for corpus indexing
  query     Placeholder for SWOG LIST queries
  fix       Placeholder for strict checks and autofixes
  capture   Placeholder for template capture
  check     Placeholder alias for strict checking

Options:
  -h, --help     Print help
  -V, --version  Print version

Epic 1 provides the executable workspace skeleton only. Parser, store, query,
capture, and fix behavior are intentionally pending."
    );
}
