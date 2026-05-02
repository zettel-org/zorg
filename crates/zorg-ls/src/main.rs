use std::env;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("-V" | "--version") => println!("zorg-ls {VERSION}"),
        Some("-h" | "--help") | None => print_help(),
        Some(argument) => {
            eprintln!("unsupported zorg-ls argument: {argument}");
            eprintln!("run `zorg-ls --help` for usage");
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!(
        "\
zorg-ls {VERSION}

Usage: zorg-ls [OPTIONS]

Options:
  -h, --help     Print help
  -V, --version  Print version

Epic 1 provides the executable language-server placeholder only. LSP protocol
handling is intentionally pending."
    );
}
