// Copyright 2020-2024 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use clap_complete::ArgValueCandidates;
use std::io::Write as _;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use tracing::instrument;

use thiserror::Error;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::command_error::user_error;
use crate::command_error::CommandError;
use crate::complete;
use crate::ui::Ui;

/// Generate a link to a commit in a GitHub repository.
///
/// Generates a GitHub link to the commit corresponding to the given revision.
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct GithubLinkArgs {
    /// an optional revision to generate a link to
    #[arg(long, short, add = ArgValueCandidates::new(complete::all_revisions))]
    revision: Option<RevisionArg>,
}

#[instrument(skip_all)]
pub(crate) fn cmd_github_link(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &GithubLinkArgs,
) -> Result<(), CommandError> {
    let workspace_command = command.workspace_helper(ui)?;
    let commit = workspace_command
        .resolve_single_rev(ui, args.revision.as_ref().unwrap_or(&RevisionArg::AT))?;

    let mut cmd = gh_browse_command();
    cmd.arg(format!("{}", commit.id()));

    let result = run_command(&mut cmd)
        .map(|res| parse_utf8_string(res))
        .and_then(|res| res.map_err(Into::into));

    match result {
        Ok(url) => {
            writeln!(ui.stdout(), "{}", url.trim_end())?;
            Ok(())
        }
        Err(err) => {
            return Err(err.into());
        }
    }
}

fn run_command(command: &mut Command) -> GhResult<Vec<u8>> {
    tracing::info!(?command, "running gh command");
    let process = command.spawn()?;
    let output = process.wait_with_output()?;
    tracing::info!(?command, ?output.status, "gh command exited");
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(GhError::Command {
            exit_status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim_end().into(),
        })
    }
}

fn gh_browse_command() -> Command {
    let mut command = Command::new("gh");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("browse")
        .arg("--no-browser");

    command
}

type GhResult<T> = Result<T, GhError>;

fn parse_utf8_string(data: Vec<u8>) -> GhResult<String> {
    String::from_utf8(data).map_err(|_| GhError::BadResult)
}

#[derive(Debug, Error)]
pub enum GhError {
    #[error("gh command failed with {exit_status}:\n{stderr}")]
    Command {
        exit_status: ExitStatus,
        stderr: String,
    },
    #[error("Failed to parse response from gh")]
    BadResult,
    #[error("Failed to run gh")]
    Io(#[from] std::io::Error),
}

impl Into<CommandError> for GhError {
    fn into(self) -> CommandError {
        match self {
            GhError::Command {
                exit_status,
                stderr,
            } => user_error(format!(
                "gh command failed with {}: {}",
                exit_status, stderr,
            )),
            GhError::BadResult => user_error("Failed to parse response from gh"),
            GhError::Io(_) => user_error("Failed to run gh. Is the gh CLI  installed?"),
        }
    }
}
