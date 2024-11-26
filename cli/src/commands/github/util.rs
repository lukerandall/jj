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

use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;

use thiserror::Error;

use crate::command_error::user_error;
use crate::command_error::user_error_with_hint;
use crate::command_error::CommandError;

pub fn gh_command() -> Command {
    let mut command = Command::new("gh");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    command
}

pub fn run_command_with_output(command: &mut Command) -> GhResult<String> {
    let output = run_command(command)?;
    let output = parse_utf8_string(output)?.trim_end().to_string();

    Ok(output)
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

impl From<GhError> for CommandError {
    fn from(error: GhError) -> CommandError {
        match error {
            GhError::Command {
                exit_status,
                stderr,
            } => user_error(format!(
                "gh command failed with {}: {}",
                exit_status, stderr,
            )),
            GhError::BadResult => user_error("Failed to parse response from gh"),
            GhError::Io(err) => user_error_with_hint(
                format!("gh failed with error: {}", err),
                "Check the gh CLI is installed and `gh auth status` shows that you are logged in",
            ),
        }
    }
}
