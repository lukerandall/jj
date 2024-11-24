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

pub mod link;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::ui::Ui;

/// GitHub operations.
#[derive(clap::Subcommand, Clone, Debug)]
pub enum GithubCommand {
    Link(link::GithubLinkArgs),
}

pub fn cmd_github(
    ui: &mut Ui,
    command: &CommandHelper,
    subcommand: &GithubCommand,
) -> Result<(), CommandError> {
    match subcommand {
        GithubCommand::Link(args) => link::cmd_github_link(ui, command, args),
    }
}
