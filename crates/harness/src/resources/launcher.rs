//! Small host launcher, compiled independently of the workspace dependency graph.
mod measure;

#[cfg(test)]
mod tests;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--container") {
        args.remove(0);
        std::process::exit(measure::container(&args).unwrap_or_else(|error| {
            eprintln!("resource observer: {error}");
            1
        }));
    }
    if let Err(error) = measure::run(args) {
        eprintln!("resource measurement: {error}");
        std::process::exit(1);
    }
}
