use aoe_geodata::{WorkerRequest, execute};
use std::{ffi::OsString, io::Read, process::ExitCode};

mod cli;

const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<OsString>>();
    let result = if arguments.is_empty() {
        run_worker()
    } else {
        cli::run(&arguments)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("aoe-map-worker: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_worker() -> Result<(), String> {
    let mut bytes = Vec::with_capacity(MAX_REQUEST_BYTES as usize + 1);
    std::io::stdin()
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read worker request: {error}"))?;
    if bytes.len() > MAX_REQUEST_BYTES as usize {
        return Err(format!("worker request exceeds {MAX_REQUEST_BYTES} bytes"));
    }
    let request = serde_json::from_slice::<WorkerRequest>(&bytes)
        .map_err(|error| format!("invalid worker request: {error}"))?;
    let response = execute(request).map_err(|error| error.to_string())?;
    let output = serde_json::to_vec(&response)
        .map_err(|error| format!("could not encode worker response: {error}"))?;
    if output.len() > MAX_RESPONSE_BYTES {
        return Err(format!(
            "worker response exceeds {MAX_RESPONSE_BYTES} bytes"
        ));
    }
    std::io::Write::write_all(&mut std::io::stdout(), &output)
        .map_err(|error| format!("could not write worker response: {error}"))?;
    Ok(())
}
