use aoe_geodata::{WorkerRequest, execute};
use std::{io::Read, process::ExitCode};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("aoe-map-worker: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
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
    let output = serde_json::to_string(&response)
        .map_err(|error| format!("could not encode worker response: {error}"))?;
    println!("{output}");
    Ok(())
}
