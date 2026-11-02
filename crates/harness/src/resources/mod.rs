//! Host-side container resource policy and qualification.
pub(crate) mod policy;
pub(crate) fn docker_command<T: AsRef<str>>(args: &[T]) -> std::io::Result<std::process::Command> {
    let mut command = std::process::Command::new("docker");
    command.args(docker_args(args)?);
    crate::jobs::guard::prepare_child(&mut command)?;
    Ok(command)
}
/// Direct Docker launchers use the same admission and caps as supervised ones.
pub(crate) fn docker_args<T: AsRef<str>>(args: &[T]) -> std::io::Result<Vec<String>> {
    let mut args: Vec<String> = args.iter().map(|arg| arg.as_ref().to_owned()).collect();
    if args.first().is_some_and(|arg| arg == "run") {
        policy::check()?;
        if !args.iter().any(|arg| arg.starts_with("--memory=")) {
            args.splice(1..1, policy::cap_args());
        }
    }
    Ok(args)
}
pub(crate) fn before_docker(command: &std::process::Command) -> std::io::Result<()> {
    if std::path::Path::new(command.get_program())
        .file_name()
        .is_some_and(|name| name == "docker")
        && command.get_args().next().is_some_and(|arg| arg == "run")
    {
        policy::check()?;
    }
    Ok(())
}
#[cfg(test)]
mod measure;
#[cfg(test)]
mod tests;
