use herdr_planbook::process::capture;
use std::{
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn a_stuck_child_is_terminated_and_reaped() {
    let started = Instant::now();
    let error = capture(Command::new("sleep").arg("2"), Duration::from_millis(20)).unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(1));
}
#[test]
fn output_larger_than_a_pipe_does_not_block_waiting() {
    let text = "a".repeat(70000);
    let output = capture(
        Command::new("printf").arg("%s").arg(&text),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, text.as_bytes());
}
