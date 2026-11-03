//! Small host launcher, compiled independently of the workspace dependency graph.
mod observer;
mod policy;

#[cfg(test)]
mod tests;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--run") {
        args.remove(0);
        std::process::exit(run(&args).unwrap_or_else(|error| {
            eprintln!("resource admission: {error}");
            76
        }));
    }
    if args.first().is_some_and(|arg| arg == "--container") {
        args.remove(0);
        std::process::exit(observer::container(&args).unwrap_or_else(|error| {
            eprintln!("resource observer: {error}");
            1
        }));
    }
    eprintln!("resource runner: expected --run or --container followed by a command");
    std::process::exit(1);
}

fn run(args: &[String]) -> std::io::Result<i32> {
    let (program, args) = args
        .split_first()
        .ok_or_else(|| std::io::Error::other("resource run: missing command"))?;
    policy::check()?;
    let status = std::process::Command::new(program).args(args).status()?;
    let code = status.code().unwrap_or(137);
    if code == 137 {
        eprintln!(
            "resource: command killed (exit 137); container caps: {}; a kernel OOM kill means the memory cap was exceeded",
            policy::cap_args().join(" ")
        );
    }
    Ok(code)
}
