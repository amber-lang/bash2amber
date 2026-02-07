use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use bash2amber::convert_bash_to_amber;
use test_generator::test_resources;

static RUN_ID: AtomicU64 = AtomicU64::new(1);
// Keep this file changing when fixtures are added so test-generator re-expands glob inputs. (updated for function_echo_simple)

#[test_resources("tests/bash/*.sh")]
fn converts_fixture(resource: &str) {
    let source = fs::read_to_string(resource)
        .unwrap_or_else(|err| panic!("Failed to read '{resource}': {err}"));

    let output = convert_bash_to_amber(&source, Some(resource.to_string()))
        .unwrap_or_else(|err| panic!("Failed to convert '{resource}': {err}"));

    let expected_path = expected_path_for(resource);
    let expected = fs::read_to_string(&expected_path).unwrap_or_else(|err| {
        panic!(
            "Failed to read expected output '{}': {err}",
            expected_path.display()
        )
    });

    assert_eq!(
        expected,
        output,
        "Converted output mismatch for '{}'. Expected file: '{}'.",
        resource,
        expected_path.display()
    );

    if should_compare_runtime_output(&source) {
        assert_same_runtime_output(resource, &output);
    }
}

fn expected_path_for(resource: &str) -> PathBuf {
    let input = Path::new(resource);
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("Invalid input fixture filename");
    Path::new("tests/amber").join(format!("{stem}.ab"))
}

fn assert_same_runtime_output(resource: &str, amber_source: &str) {
    let bash_result = run_bash_script(Path::new(resource));
    let amber_result = run_generated_amber_script(resource, amber_source);
    let bash_stdout = bash_result.stdout.clone();
    let amber_stdout = amber_result.stdout.clone();
    let bash_stderr = normalize_stderr(&bash_result.stderr);
    let amber_stderr = normalize_stderr(&amber_result.stderr);

    assert_eq!(
        bash_result.status_code, amber_result.status_code,
        "Exit code mismatch for fixture '{resource}'"
    );
    assert_eq!(
        bash_stdout, amber_stdout,
        "Stdout mismatch for fixture '{resource}'"
    );
    assert_eq!(
        bash_stderr, amber_stderr,
        "Stderr mismatch for fixture '{resource}'"
    );
}

#[derive(Debug)]
struct RunResult {
    status_code: i32,
    stdout: String,
    stderr: String,
}

fn run_bash_script(script_path: &Path) -> RunResult {
    let mut cmd = Command::new("bash");
    cmd.arg(script_path);
    cmd.current_dir(manifest_dir());
    run_command(cmd, &format!("bash {}", script_path.display()))
}

fn run_generated_amber_script(resource: &str, amber_source: &str) -> RunResult {
    let root_dir = manifest_dir();
    let runtime_dir = root_dir.join("target/runtime-compare");
    fs::create_dir_all(&runtime_dir).unwrap_or_else(|err| {
        panic!(
            "Failed to create runtime comparison directory '{}': {err}",
            runtime_dir.display()
        )
    });

    let stem = Path::new(resource)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("fixture");
    let id = RUN_ID.fetch_add(1, Ordering::Relaxed);

    let amber_file = runtime_dir.join(format!("{stem}_{id}.ab"));
    let shell_file = runtime_dir.join(format!("{stem}_{id}.sh"));

    fs::write(&amber_file, amber_source).unwrap_or_else(|err| {
        panic!(
            "Failed to write generated Amber file '{}': {err}",
            amber_file.display()
        )
    });

    let mut build_cmd = Command::new("amber");
    build_cmd
        .arg("build")
        .arg(&amber_file)
        .arg(&shell_file)
        .current_dir(&root_dir);

    let build = run_raw_output(
        build_cmd,
        &format!(
            "amber build {} {}",
            amber_file.display(),
            shell_file.display()
        ),
    );
    if !build.status.success() {
        panic!(
            "Failed to build generated Amber fixture '{}'.\\nstdout:\\n{}\\nstderr:\\n{}",
            amber_file.display(),
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
    }

    let mut run_cmd = Command::new("bash");
    run_cmd.arg(&shell_file).current_dir(root_dir);
    run_command(run_cmd, &format!("bash {}", shell_file.display()))
}

fn run_command(command: Command, description: &str) -> RunResult {
    let output = run_raw_output(command, description);
    RunResult {
        status_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn run_raw_output(mut command: Command, description: &str) -> Output {
    command.output().unwrap_or_else(|err| {
        panic!("Failed to run command '{description}': {err}");
    })
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn normalize_stderr(stderr: &str) -> String {
    let mut normalized = Vec::new();
    for line in stderr.lines() {
        normalized.push(normalize_stderr_line(line));
    }
    if normalized.is_empty() {
        String::new()
    } else {
        normalized.join("\n") + "\n"
    }
}

fn normalize_stderr_line(line: &str) -> String {
    if let Some(pos) = line.find(": line ") {
        let after = &line[pos + ": line ".len()..];
        if let Some(colon_space) = after.find(": ") {
            return after[colon_space + 2..].to_string();
        }
    }
    line.to_string()
}

fn should_compare_runtime_output(source: &str) -> bool {
    !source.lines().any(|line| line.trim() == "### No execute")
}
