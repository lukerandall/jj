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
use jj_lib::refs::LocalAndRemoteRef;
use jj_lib::str_util::StringPattern;
use jj_lib::view::View;
use thiserror::Error;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::command_error::user_error;
use crate::command_error::user_error_with_hint;
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
    #[arg(long, short, conflicts_with = "bookmark", add = ArgValueCandidates::new(complete::all_revisions))]
    revision: Vec<RevisionArg>,

    /// Optional bookmark to generate link to; requires that the bookmark
    /// exists as a branch in the remote repository.
    ///
    /// By default, the specified name matches exactly. Use `glob:` prefix to
    /// select bookmarks by wildcard pattern. For details, see
    /// https://martinvonz.github.io/jj/latest/revsets/#string-patterns.
    #[arg(
        long, short,
        conflicts_with = "revision",
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
    if let Some(pattern) = args.bookmark.as_ref() {
        let repo = workspace_command.repo();
        // TODO: determine which remote to use
        let bookmarks = find_bookmarks(&repo.view(), &pattern, "origin")?;
        links = links_for_bookmarks(bookmarks.iter().map(|(name, _)| name.to_string()).collect())?;
    } else if args.revision.is_empty() {
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

fn generate_links<T, F>(items: Vec<T>, configure_command: F) -> Result<Vec<String>, CommandError>
where
    F: Fn(&mut Command, T),
{
    items
        .into_iter()
        .map(|item| generate_link(item, &configure_command))
        .collect()
}

fn generate_link<T, F>(item: T, configure_command: &F) -> Result<String, CommandError>
where
    F: Fn(&mut Command, T),
{
    let mut cmd = gh_browse_command();
    configure_command(&mut cmd, item);

    let output = run_command(&mut cmd)?;
    let url = parse_utf8_string(output)?.trim_end().to_string();

    Ok(url)
}

fn links_for_commits(commits: Vec<Commit>) -> Result<Vec<String>, CommandError> {
    generate_links(commits, |cmd, commit| {
        cmd.arg(format!("{}", commit.id()));
    })
}

fn links_for_bookmarks(bookmarks: Vec<String>) -> Result<Vec<String>, CommandError> {
    generate_links(bookmarks, |cmd, bookmark| {
        cmd.arg("--branch").arg(format!("{}", bookmark));
    })
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

fn find_bookmarks<'a>(
    view: &'a View,
    bookmark_patterns: &[StringPattern],
    remote_name: &str,
) -> Result<Vec<(&'a str, LocalAndRemoteRef<'a>)>, CommandError> {
    let mut matching_bookmarks = vec![];
    let mut unmatched_patterns = vec![];
    for pattern in bookmark_patterns {
        let mut matches = view
            .local_remote_bookmarks_matching(pattern, remote_name)
            .filter(|(_, targets)| {
                // If the remote exists but is not tracking, the absent local shouldn't
                // be considered a deleted bookmark.
                targets.local_target.is_present() || targets.remote_ref.is_tracking()
            })
            .peekable();
        if matches.peek().is_none() {
            unmatched_patterns.push(pattern);
        }
        matching_bookmarks.extend(matches);
    }
    match &unmatched_patterns[..] {
        [] => Ok(matching_bookmarks),
        [pattern] if pattern.is_exact() => Err(user_error(format!("No such bookmark: {pattern}"))),
        patterns => Err(user_error(format!(
            "No matching bookmarks for patterns: {}",
            patterns.iter().join(", ")
        ))),
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
