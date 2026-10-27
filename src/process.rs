//! Bounded subprocesses with argv invocation and no shell.
use anyhow::{Context, Result, bail};
use std::{
    io::{Read, Seek, Write},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

pub fn capture(command: &mut Command, timeout: Duration) -> Result<Output> {
    capture_input(command, timeout, None)
}

pub fn capture_input(
    command: &mut Command,
    timeout: Duration,
    input: Option<&[u8]>,
) -> Result<Output> {
    let stdin = if let Some(input) = input {
        let mut file = tempfile::tempfile()?;
        file.write_all(input)?;
        file.rewind()?;
        Stdio::from(file)
    } else {
        Stdio::null()
    };
    // Files prevent a full stdout pipe from blocking the timeout wait.
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = command
        .stdin(stdin)
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()
        .context("Cannot start command")?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                bail!("Command timed out after {} seconds", timeout.as_secs());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
    };
    fn contents(file: &mut std::fs::File) -> Result<Vec<u8>> {
        file.rewind()?;
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        anyhow::ensure!(bytes.len() <= 1024 * 1024, "Command output exceeds 1 MiB");
        Ok(bytes)
    }
    Ok(Output {
        status,
        stdout: contents(&mut stdout)?,
        stderr: contents(&mut stderr)?,
    })
}
