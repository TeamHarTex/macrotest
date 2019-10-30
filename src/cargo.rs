use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::expand::Project;
use crate::manifest::Name;
use crate::rustflags;
use std::io::BufRead;

#[derive(Deserialize)]
pub struct Metadata {
    pub target_directory: PathBuf,
    pub workspace_root: PathBuf,
}

fn raw_cargo() -> Command {
    Command::new(option_env!("CARGO").unwrap_or("cargo"))
}

fn cargo(project: &Project) -> Command {
    let mut cmd = raw_cargo();
    cmd.current_dir(&project.dir);
    cmd.env(
        "CARGO_TARGET_DIR",
        path!(project.target_dir / "tests" / "target"),
    );
    rustflags::set_env(&mut cmd);
    cmd
}

pub(crate) fn metadata() -> Result<Metadata> {
    let output = raw_cargo()
        .arg("metadata")
        .arg("--format-version=1")
        .output()
        .map_err(Error::Cargo)?;

    serde_json::from_slice(&output.stdout).map_err(Error::CargoMetadata)
}

pub(crate) fn expand(project: &Project, name: &Name) -> Result<(bool, Vec<u8>)> {
    let cargo_expand = cargo(project)
        .arg("expand")
        .arg("--bin")
        .arg(name.as_ref())
        .arg("--theme")
        .arg("none")
        .output()
        .map_err(|e| Error::CargoExpandExecutionError(e.to_string()))?;

    if !cargo_expand.status.success() {
        return Ok((false, cargo_expand.stderr));
    }

    Ok((true, cargo_expand.stdout))
}

/// Builds dependencies for macro expansion and pipes `cargo` output to `STDOUT`.
/// Tries to expand macros in `main.rs` and intentionally filters the result.
/// This function is called before macro expansions to speed them up and
/// for dependencies build process to be visible for user.
pub(crate) fn build_dependencies(project: &Project) -> Result<()> {
    use std::io::Write;

    let stdout = cargo(project)
        .arg("expand")
        .arg("--bin")
        .arg(project.name.clone())
        .arg("--theme")
        .arg("none")
        .stdout(std::process::Stdio::piped())
        .spawn()?
        .stdout
        .ok_or(Error::CargoFail)?;

    let reader = std::io::BufReader::new(stdout);

    // Filter ignored lines and main.rs content
    reader
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.starts_with("fn main() {}"))
        .filter(|line| !line_should_be_ignored(line))
        .for_each(|line| {
            let _ = write!(std::io::stdout(), "{}\n", line);
        });

    Ok(())
}

const IGNORED_LINES: [&'static str; 5] = [
    "#![feature(prelude_import)]",
    "#[prelude_import]",
    "use std::prelude::v1::*;",
    "#[macro_use]",
    "extern crate std;",
];

fn line_should_be_ignored(line: &str) -> bool {
    for check in IGNORED_LINES.iter() {
        if line.starts_with(check) {
            return true;
        }
    }

    false
}