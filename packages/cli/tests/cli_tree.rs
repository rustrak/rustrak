//! The command tree itself, which is where two bugs an integration test cannot
//! catch at runtime show up: clap builds the whole tree and asserts on it, so a
//! duplicate id or a bad positional order panics *before* any command runs.
//!
//! Each case here was a real failure:
//! - a positional called `project` colliding with the global `--project`;
//! - a positional called `version` colliding with clap's auto `--version`;
//! - `api`'s optional method before its required path.

use clap::{CommandFactory, Parser};

use rustrak_cli::cli::{Cli, Command, ProjectCommand, ReleaseCommand};

/// `debug_assert!`s run in a test build, which is what catches a malformed
/// tree. `--version` and `--help` are also parsed here, so an id collision
/// with either is a failure.
fn assert_tree_is_valid() {
    Cli::command().debug_assert();
}

#[test]
fn the_command_tree_builds_without_a_clap_assertion() {
    assert_tree_is_valid();
}

#[test]
fn the_help_and_version_flags_parse() {
    Cli::try_parse_from(["rustrak", "--version"]).unwrap_err();
    Cli::try_parse_from(["rustrak", "--help"]).unwrap_err();
}

#[test]
fn every_subcommands_help_builds() {
    // `debug_assert` walks the whole tree, but building each subcommand on its
    // own is what catches an id that only clashes within that one.
    for name in [
        "auth",
        "config",
        "project",
        "issue",
        "event",
        "log",
        "trace",
        "perf",
        "release",
        "monitor",
        "alert",
        "team",
        "token",
        "send",
        "api",
        "completion",
    ] {
        Cli::try_parse_from(["rustrak", name, "--help"]).unwrap_err();
    }
}

#[test]
fn a_project_accepts_both_the_flag_and_the_positional() {
    // The bug: `--project 1` was rejected because the positional shared the
    // id the global flag uses.
    let cli = Cli::parse_from(["rustrak", "project", "get", "--project", "1"]);
    let Command::Project(ProjectCommand::Get(args)) = cli.command else {
        panic!("expected project get");
    };
    assert!(args.project_ref.is_none());
    assert_eq!(cli.global.project.as_deref(), Some("1"));

    let cli = Cli::parse_from(["rustrak", "project", "get", "2"]);
    let Command::Project(ProjectCommand::Get(args)) = cli.command else {
        panic!("expected project get");
    };
    assert_eq!(args.project_ref.as_deref(), Some("2"));
}

#[test]
fn a_release_version_argument_does_not_clash_with_the_version_flag() {
    let cli = Cli::parse_from(["rustrak", "release", "get", "1.4.2"]);
    let Command::Release(ReleaseCommand::Get(args)) = cli.command else {
        panic!("expected release get");
    };
    assert_eq!(args.release_version, "1.4.2");
}

#[test]
fn api_takes_the_path_first_and_the_method_as_a_flag() {
    let cli = Cli::parse_from(["rustrak", "api", "/api/projects"]);
    let Command::Api(args) = cli.command else {
        panic!("expected api");
    };
    assert_eq!(args.path, "/api/projects");
    assert!(matches!(args.method, rustrak_cli::cli::Method::Get));

    let cli = Cli::parse_from(["rustrak", "api", "/api/projects", "--method", "post"]);
    let Command::Api(args) = cli.command else {
        panic!("expected api");
    };
    assert!(matches!(args.method, rustrak_cli::cli::Method::Post));
}

#[test]
fn the_json_and_table_flags_are_mutually_exclusive() {
    assert!(Cli::try_parse_from(["rustrak", "issue", "list", "--json", "--table"]).is_err());
}

#[test]
fn a_bare_invocation_asks_for_a_command() {
    assert!(Cli::try_parse_from(["rustrak"]).is_err());
}
