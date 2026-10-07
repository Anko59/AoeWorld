//! `curl` and `wget`: where they write, and for agents, that they reach
//! localhost only.
use crate::agents::{
    args::{self, Spec},
    context::{Access, Context, Verdict},
    shell::Word,
};
use std::path::Path;

fn local_url(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let host = if host.starts_with('[') {
        host.split(']').next().map(|h| &h[1..]).unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or_default()
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
}

pub(super) fn curl(context: &Context, cwd: Option<&Path>, rest: &[Word]) -> Verdict {
    let parsed = args::split(
        rest,
        &Spec {
            short: "oDcKeHdXuAbFTwmxrCEyYzQtPU",
            long: &[
                "output",
                "output-dir",
                "dump-header",
                "cookie-jar",
                "trace",
                "trace-ascii",
                "stderr",
                "libcurl",
                "config",
                "url",
                "data",
                "header",
                "request",
                "user",
                "user-agent",
                "cookie",
                "form",
                "upload-file",
                "write-out",
                "max-time",
                "proxy",
                "range",
                "referer",
                "cert",
                "data-binary",
                "data-raw",
                "json",
            ],
            stop_at_positional: false,
        },
    );
    let agent = context.role.is_agent();
    if agent
        && parsed.has(&[
            "-O",
            "--remote-name",
            "--remote-name-all",
            "-J",
            "--remote-header-name",
            "-K",
            "--config",
        ])
    {
        return Err(
            "agents name curl's output with `-o <path>`; remote names and config files are opaque"
                .into(),
        );
    }
    if agent {
        let urls = parsed
            .positionals
            .iter()
            .copied()
            .chain(parsed.values(&["--url"]));
        if let Some(url) = urls.into_iter().find(|w| !w.plain() || !local_url(&w.text)) {
            return Err(format!("agents reach localhost only, not `{}`", url.text));
        }
    }
    parsed
        .values(&[
            "-o",
            "--output",
            "-D",
            "--dump-header",
            "-c",
            "--cookie-jar",
            "--trace",
            "--trace-ascii",
            "--stderr",
            "--libcurl",
        ])
        .filter(|w| w.text != "-")
        .try_for_each(|w| context.write(cwd, w, Access::Put))?;
    parsed
        .values(&["--output-dir"])
        .try_for_each(|w| context.write(cwd, w, Access::Tree))
}

pub(super) fn wget(context: &Context, cwd: Option<&Path>, rest: &[Word]) -> Verdict {
    let parsed = args::split(
        rest,
        &Spec {
            short: "OoaPiUeT",
            long: &[
                "output-document",
                "output-file",
                "append-output",
                "directory-prefix",
                "input-file",
                "user-agent",
                "execute",
                "timeout",
            ],
            stop_at_positional: false,
        },
    );
    if context.role.is_agent() {
        if parsed.has(&["-i", "--input-file", "-e", "--execute"]) {
            return Err("wget input lists and `-e` commands are opaque".into());
        }
        if let Some(url) = parsed
            .positionals
            .iter()
            .find(|w| !w.plain() || !local_url(&w.text))
        {
            return Err(format!("agents reach localhost only, not `{}`", url.text));
        }
    }
    let documents: Vec<&Word> = parsed.values(&["-O", "--output-document"]).collect();
    documents
        .iter()
        .filter(|w| w.text != "-")
        .try_for_each(|w| context.write(cwd, w, Access::Put))?;
    parsed
        .values(&["-o", "--output-file", "-a", "--append-output"])
        .try_for_each(|w| context.write(cwd, w, Access::Put))?;
    let prefix = parsed.values(&["-P", "--directory-prefix"]).next().cloned();
    if documents.is_empty() {
        context.write(
            cwd,
            &prefix.unwrap_or_else(|| Word::literal(".")),
            Access::Tree,
        )?;
    }
    Ok(())
}
