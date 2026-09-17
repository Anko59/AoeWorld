//! Restricted MCP stdio interface for exploratory browser QA.
use crate::qa::{self, Finding, Journey, Report, Status};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

const VERSION: &str = "2025-11-25";
const BROWSER_ACTIONS: [&str; 9] = [
    "open_session",
    "observe",
    "activate",
    "select_scenario",
    "canvas_input",
    "wait_text",
    "screenshot",
    "diagnostics",
    "close_session",
];

struct Worker {
    process: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    name: String,
}

trait BrowserWorker {
    fn call(&mut self, name: &str, args: &Value) -> Result<Value, Box<dyn Error>>;
}

impl Worker {
    fn start() -> Result<Self, Box<dyn Error>> {
        let base =
            std::env::var("AOE_QA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
        Self::start_at(&base)
    }

    fn start_at(base: &str) -> Result<Self, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let name = format!("aoeworld-qa-{}", std::process::id());
        let parsed = url::Url::parse(base)?;
        if parsed.scheme() != "http"
            || !matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"))
            || parsed.port().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err("QA target must be a local HTTP endpoint".into());
        }
        let user = format!(
            "{}:{}",
            nix::unistd::Uid::current(),
            nix::unistd::Gid::current()
        );
        let mount = format!("{}:{}", root.display(), root.display());
        let home = format!("HOME={}/.cache/browser-home", root.display());
        let mut process = Command::new("docker")
            .args([
                "run",
                "--rm",
                "--init",
                "-i",
                "--network",
                "host",
                "--ipc",
                "host",
                "--user",
                &user,
                "--name",
                &name,
                "-e",
                &home,
                "-e",
                "AOE_QA_BASE_URL",
                "-v",
                &mount,
                "-w",
            ])
            .arg(root.join("browser"))
            .args([
                "aoeworld/browser-tools:1.63.0",
                "xvfb-run",
                "-a",
                "node",
                "qa-worker.mjs",
            ])
            .env("AOE_QA_BASE_URL", base)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = process.stdin.take().ok_or("QA worker stdin missing")?;
        let output = BufReader::new(process.stdout.take().ok_or("QA worker stdout missing")?);
        Ok(Self {
            process,
            input,
            output,
            name,
        })
    }
}

impl BrowserWorker for Worker {
    fn call(&mut self, name: &str, args: &Value) -> Result<Value, Box<dyn Error>> {
        exchange(&mut self.input, &mut self.output, name, args)
    }
}

fn exchange<W: Write, R: BufRead>(
    input: &mut W,
    output: &mut R,
    name: &str,
    args: &Value,
) -> Result<Value, Box<dyn Error>> {
    writeln!(input, "{}", json!({"name": name, "args": args}))?;
    input.flush()?;
    let mut line = String::new();
    if output.read_line(&mut line)? == 0 {
        return Err("QA worker closed".into());
    }
    let result: Value = serde_json::from_str(&line)?;
    if result["ok"] != true {
        return Err(result["error"]
            .as_str()
            .unwrap_or("QA worker failed")
            .to_owned()
            .into());
    }
    Ok(result["result"].clone())
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

struct Server {
    start: Instant,
    budget: Duration,
    worker: Option<Box<dyn BrowserWorker>>,
    evidence_dir: PathBuf,
    report: Report,
}

fn required<'a>(args: &'a Value, key: &str, limit: usize) -> Result<&'a str, String> {
    let value = args[key]
        .as_str()
        .ok_or_else(|| format!("{key} is required"))?;
    if value.is_empty() || value.len() > limit {
        return Err(format!("invalid {key} length"));
    }
    Ok(value)
}

fn validate_action(name: &str, args: &Value) -> Result<(), String> {
    let session = required(args, "session", 32)?;
    if !session
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("invalid session name".into());
    }
    match name {
        "open_session" => {
            if !args["capability"].is_null() && args["capability"] != "webgpu-disabled" {
                return Err("unsupported capability mode".into());
            }
            if !args["configuration"].is_null() && args["configuration"] != "invalid-scenario" {
                return Err("unsupported configuration mode".into());
            }
        }
        "activate" => {
            let role = required(args, "role", 20)?;
            if !["button", "link"].contains(&role) {
                return Err("role is not allowed".into());
            }
            required(args, "label", 80)?;
        }
        "select_scenario" => {
            if !["smoke", "target-distributed", "target-hotspot"]
                .contains(&required(args, "scenario", 40)?)
            {
                return Err("scenario is not selectable".into());
            }
        }
        "canvas_input" => {
            let action = required(args, "action", 10)?;
            if action == "key" {
                if ![
                    "ArrowUp",
                    "ArrowDown",
                    "ArrowLeft",
                    "ArrowRight",
                    "w",
                    "a",
                    "s",
                    "d",
                    "+",
                    "-",
                    "Space",
                ]
                .contains(&required(args, "key", 12)?)
                {
                    return Err("key is not allowed".into());
                }
            } else if ["click", "move", "wheel"].contains(&action) {
                let x = args["x"].as_i64().ok_or("x is required")?;
                let y = args["y"].as_i64().ok_or("y is required")?;
                if !(0..=1280).contains(&x) || !(0..=720).contains(&y) {
                    return Err("canvas coordinates exceed viewport".into());
                }
                if action == "wheel"
                    && !(-1000..=1000).contains(&args["delta"].as_i64().ok_or("delta is required")?)
                {
                    return Err("wheel delta exceeds limit".into());
                }
            } else {
                return Err("canvas action is not allowed".into());
            }
        }
        "wait_text" => {
            required(args, "text", 100)?;
            if !(1..=10000).contains(
                &args["timeout_ms"]
                    .as_u64()
                    .ok_or("timeout_ms is required")?,
            ) {
                return Err("wait deadline exceeds limit".into());
            }
        }
        _ => {}
    }
    Ok(())
}

impl Server {
    fn new(budget: &str) -> Result<Self, Box<dyn Error>> {
        Self::new_at(budget, PathBuf::from("reports/qa"))
    }

    fn new_at(budget: &str, evidence_dir: PathBuf) -> Result<Self, Box<dyn Error>> {
        let minutes = match budget {
            "fast" => 15,
            "full" => 45,
            "extended" => 120,
            _ => return Err("budget must be fast, full, or extended".into()),
        };
        Ok(Self {
            start: Instant::now(),
            budget: Duration::from_secs(minutes * 60),
            worker: None,
            evidence_dir,
            report: Report {
                version: 1,
                budget: budget.to_owned(),
                build: String::new(),
                scenario: String::new(),
                status: Status::Blocked,
                journeys: Vec::new(),
                findings: Vec::new(),
            },
        })
    }

    fn tool_call(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        if self.start.elapsed() > self.budget {
            return Err("QA time budget exhausted".into());
        }
        if BROWSER_ACTIONS.contains(&name) {
            validate_action(name, args)?;
            if self.worker.is_none() {
                self.worker = Some(Box::new(
                    Worker::start().map_err(|error| error.to_string())?,
                ));
            }
            let result = self
                .worker
                .as_mut()
                .ok_or("QA worker unavailable")?
                .call(name, args)
                .map_err(|error| error.to_string())?;
            if name == "open_session" {
                self.report.build = result["build"].as_str().unwrap_or("").to_owned();
                self.report.scenario = result["scenario"].as_str().unwrap_or("").to_owned();
            }
            return Ok(result);
        }
        match name {
            "record_journey" => {
                let journey = required(args, "journey", 50)?;
                if !qa::REQUIRED.contains(&journey) {
                    return Err("unknown required journey".into());
                }
                let evidence = required(args, "evidence", 256)?;
                qa::validate_evidence_at(&self.evidence_dir, Path::new(evidence))?;
                if self.report.journeys.iter().any(|item| item.name == journey) {
                    return Err("journey already recorded".into());
                }
                self.report.journeys.push(Journey {
                    name: journey.to_owned(),
                    completed: true,
                    evidence: vec![evidence.to_owned()],
                });
                Ok(json!({"recorded": journey}))
            }
            "record_finding" => {
                let evidence = required(args, "evidence", 256)?;
                qa::validate_evidence_at(&self.evidence_dir, Path::new(evidence))?;
                let finding = Finding {
                    title: required(args, "title", 100)?.to_owned(),
                    reproduction: required(args, "reproduction", 1000)?.to_owned(),
                    expected: required(args, "expected", 500)?.to_owned(),
                    actual: required(args, "actual", 500)?.to_owned(),
                    evidence: vec![evidence.to_owned()],
                };
                self.report.findings.push(finding);
                Ok(json!({"findings": self.report.findings.len()}))
            }
            "finish" => {
                self.report.status = match required(args, "status", 10)? {
                    "PASS" => Status::Pass,
                    "FINDINGS" => Status::Findings,
                    "BLOCKED" => Status::Blocked,
                    _ => return Err("invalid QA status".into()),
                };
                qa::validate(&self.report)?;
                fs::create_dir_all(&self.evidence_dir).map_err(|error| error.to_string())?;
                let path = self.evidence_dir.join("session.json");
                fs::write(
                    &path,
                    serde_json::to_vec_pretty(&self.report).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                Ok(json!({"report": path, "status": self.report.status}))
            }
            _ => Err("unknown QA tool".into()),
        }
    }
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name": name, "description": description, "inputSchema": {"type": "object", "properties": properties, "required": required, "additionalProperties": false}})
}

fn tools() -> Value {
    let session = json!({"session":{"type":"string"}});
    json!({"tools": [
        tool("open_session", "Open one isolated local browser session, optionally with a bounded capability or configuration failure", json!({"session":{"type":"string"},"capability":{"type":"string","enum":["webgpu-disabled"]},"configuration":{"type":"string","enum":["invalid-scenario"]}}), &["session"]),
        tool("observe", "Read accessible page observations and visible diagnostics", session.clone(), &["session"]),
        tool("activate", "Activate one visible button or link by exact accessible name", json!({"session":{"type":"string"},"role":{"type":"string"},"label":{"type":"string"}}), &["session","role","label"]),
        tool("select_scenario", "Select a supported scenario in the visible control", json!({"session":{"type":"string"},"scenario":{"type":"string"}}), &["session","scenario"]),
        tool("canvas_input", "Send a bounded pointer or keyboard action to the canvas", json!({"session":{"type":"string"},"action":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"},"delta":{"type":"integer"},"key":{"type":"string"}}), &["session","action"]),
        tool("wait_text", "Wait for visible text with a bounded deadline", json!({"session":{"type":"string"},"text":{"type":"string"},"timeout_ms":{"type":"integer"}}), &["session","text","timeout_ms"]),
        tool("screenshot", "Record a screenshot in the QA evidence directory", session.clone(), &["session"]),
        tool("diagnostics", "Read bounded console and network errors", session.clone(), &["session"]),
        tool("close_session", "Close one browser session", session, &["session"]),
        tool("record_journey", "Record completed required journey with evidence", json!({"journey":{"type":"string"},"evidence":{"type":"string"}}), &["journey","evidence"]),
        tool("record_finding", "Record a reproducible finding", json!({"title":{"type":"string"},"reproduction":{"type":"string"},"expected":{"type":"string"},"actual":{"type":"string"},"evidence":{"type":"string"}}), &["title","reproduction","expected","actual","evidence"]),
        tool("finish", "Validate and write PASS, FINDINGS, or BLOCKED report", json!({"status":{"type":"string"}}), &["status"])
    ]})
}

fn handle(server: &mut Server, request: &Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request["method"].as_str().unwrap_or("");
    let result = match method {
        "initialize" => Ok(
            json!({"protocolVersion": VERSION, "capabilities": {"tools": {}}, "serverInfo": {"name": "aoeworld-qa", "version": "0.1.0"}, "instructions": "Investigate the visible synthetic application; record evidence for each required journey."}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools()),
        "tools/call" => {
            let name = request["params"]["name"].as_str().unwrap_or("");
            let args = &request["params"]["arguments"];
            let outcome = server.tool_call(name, args);
            let is_error = outcome.is_err();
            let payload = match outcome {
                Ok(value) => value.to_string(),
                Err(message) => message,
            };
            Ok(json!({"content": [{"type": "text", "text": payload}], "isError": is_error}))
        }
        _ => Err(json!({"code": -32601, "message": "unknown method"})),
    };
    Some(match result {
        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
        Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
    })
}

pub fn serve(budget: &str) -> Result<(), Box<dyn Error>> {
    let mut server = Server::new(budget)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    serve_io(&mut server, stdin.lock(), &mut stdout)
}

fn serve_io<R: BufRead, W: Write>(
    server: &mut Server,
    input: R,
    output: &mut W,
) -> Result<(), Box<dyn Error>> {
    for line in input.lines() {
        let line = line?;
        if line.len() > 1_048_576 {
            return Err("MCP request exceeds 1 MiB".into());
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if let Some(response) = handle(server, &request) {
            serde_json::to_writer(&mut *output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
