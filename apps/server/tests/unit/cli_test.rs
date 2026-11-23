//! Parsing the command line: no arguments serves, as the image's CMD does.

use rustrak::cli::{parse, Command, CreateUser};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn no_arguments_serves() {
    assert_eq!(parse(args(&[])), Ok(Command::Serve));
}

#[test]
fn each_subcommand_is_recognized() {
    for (given, expected) in [
        ("--version", Command::Version),
        ("-V", Command::Version),
        ("--help", Command::Help),
        ("-h", Command::Help),
        ("check-config", Command::CheckConfig),
        ("migrate", Command::Migrate),
        ("healthcheck", Command::Healthcheck),
    ] {
        assert_eq!(parse(args(&[given])), Ok(expected), "{given}");
    }
}

#[test]
fn createuser_takes_sentrys_flags() {
    assert_eq!(
        parse(args(&[
            "createuser",
            "--email",
            "a@example.com",
            "--password",
            "secret",
            "--superuser",
            "--force-update",
        ])),
        Ok(Command::CreateUser(CreateUser {
            email: "a@example.com".into(),
            password: Some("secret".into()),
            superuser: true,
            force_update: true,
        }))
    );
    assert_eq!(
        parse(args(&["createuser", "--email=b@example.com"])),
        Ok(Command::CreateUser(CreateUser {
            email: "b@example.com".into(),
            password: None,
            superuser: false,
            force_update: false,
        }))
    );
}

#[test]
fn mistakes_are_refused_by_name() {
    for (given, names) in [
        (&["serv"][..], "serv"),
        (&["migrate", "--now"][..], "--now"),
        (&["createuser"][..], "--email"),
        (&["createuser", "--email"][..], "--email"),
        (
            &["createuser", "--email", "a@b.c", "--admin"][..],
            "--admin",
        ),
    ] {
        let error = parse(args(given)).expect_err(&format!("{given:?} must be refused"));
        assert!(error.contains(names), "{given:?}: {error}");
    }
}
