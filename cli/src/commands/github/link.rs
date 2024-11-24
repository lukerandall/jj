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

use std::io::Write as _;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use tracing::instrument;

use clap_complete::ArgValueCandidates;
use itertools::Itertools;
use jj_lib::commit::Commit;
use jj_lib::str_util::StringPattern;
use thiserror::Error;
// use jj_lib::refs::LocalAndRemoteRef;
// use jj_lib::view::View;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::command_error::user_error;
use crate::command_error::CommandError;
use crate::complete;
use crate::ui::Ui;

/// Generate a link to the GitHub repository.
///
/// Generates a GitHub link for the given revision or bookmark. If no revision
/// or bookmark is given it defaults to --revision @.
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct GithubLinkArgs {
    /// Optional revision to generate a link to
    #[arg(long, short, add = ArgValueCandidates::new(complete::all_revisions))]
    revision: Vec<RevisionArg>,

    /// Optional bookmark to generate link to; requires that the bookmark
    /// exists as a branch in the remote repository.
    ///
    /// By default, the specified name matches exactly. Use `glob:` prefix to
    /// select bookmarks by wildcard pattern. For details, see
    /// https://martinvonz.github.io/jj/latest/revsets/#string-patterns.
    #[arg(
        long, short,
        value_parser = StringPattern::parse,
        add = ArgValueCandidates::new(complete::bookmarks),
    )]
    bookmark: Option<Vec<StringPattern>>,
}

#[instrument(skip_all)]
pub(crate) fn cmd_github_link(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &GithubLinkArgs,
) -> Result<(), CommandError> {
    let workspace_command = command.workspace_helper(ui)?;

    let links: Vec<String>;
    if args.bookmark.is_some() {
        return Err(user_error("bookmarks are not yet supported"));
        // let commits = workspace_command.resolve_single_rev(ui, args.revision.as_ref().unwrap_or(&RevisionArg::AT))?;
    }
    if args.revision.is_empty() {
        let commit = workspace_command.resolve_single_rev(ui, &RevisionArg::AT)?;
        links = links_for_commits(vec![commit])?;
    } else {
        let commits = workspace_command
            .resolve_some_revsets_default_single(ui, &args.revision)?
            .into_iter()
            .collect_vec();
        links = links_for_commits(commits)?;
    }

    for link in links {
        writeln!(ui.stdout(), "{}", link)?;
    }

    Ok(())
}

fn links_for_commits(commits: Vec<Commit>) -> Result<Vec<String>, CommandError> {
    commits
        .into_iter()
        .map(|commit| {
            let mut cmd = gh_browse_command();
            cmd.arg(format!("{}", commit.id()));
            let res = run_command(&mut cmd)
                .map(|output| parse_utf8_string(output).map(|str| str.trim_end().into()));
            match res {
                Ok(Ok(url)) => Ok(url),
                Ok(Err(err)) => Err(err.into()),
                Err(err) => Err(err.into()),
            }
        })
        .collect()
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

//fn find_bookmarks<'a>(
//    view: &'a View,
//    bookmark_patterns: &[StringPattern],
//    remote_name: &str,
//) -> Result<Vec<(&'a str, LocalAndRemoteRef<'a>)>, CommandError> {
//    let mut matching_bookmarks = vec![];
//    let mut unmatched_patterns = vec![];
//    for pattern in bookmark_patterns {
//        let mut matches = view
//            .local_remote_bookmarks_matching(pattern, remote_name)
//            .filter(|(_, targets)| {
//                // If the remote exists but is not tracking, the absent local shouldn't
//                // be considered a deleted bookmark.
//                targets.local_target.is_present() || targets.remote_ref.is_tracking()
//            })
//            .peekable();
//        if matches.peek().is_none() {
//            unmatched_patterns.push(pattern);
//        }
//        matching_bookmarks.extend(matches);
//    }
//    match &unmatched_patterns[..] {
//        [] => Ok(matching_bookmarks),
//        [pattern] if pattern.is_exact() => Err(user_error(format!("No such bookmark: {pattern}"))),
//        patterns => Err(user_error(format!(
//            "No matching bookmarks for patterns: {}",
//            patterns.iter().join(", ")
//        ))),
//    }
//}

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
