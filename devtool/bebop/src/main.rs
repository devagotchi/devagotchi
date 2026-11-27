//! The `bebop` binary: the command line in `cli`, its output on stdout and stderr, and its exit
//! code (0 clean, 1 a violation, 2 a malformed rule or a usage error).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let code =
        devagotchi_bebop::cli::run(&args, &cwd, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(code);
}
