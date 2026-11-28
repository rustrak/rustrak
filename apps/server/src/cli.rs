//! The command line. No arguments serves, which is what the image runs; the
//! rest are the admin commands an operator needs when the dashboard cannot
//! help: a lost password, a broken config, a health probe in a distroless
//! image. Names and flags follow Sentry's own CLI (`sentry createuser`,
//! `sentry upgrade`) and Relay's (`relay healthcheck`).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{CreateUserRequest, User, UserRole};
use crate::services::{AuthSessionService, UsersService};

pub const USAGE: &str = "\
Usage: rustrak [COMMAND]

With no command, starts the server.

Commands:
  check-config   Read the configuration and rustrak.toml, report the first mistake
  migrate        Apply pending database migrations (the server also does at start)
  healthcheck    Exit 0 if the local server answers /health/ready, 1 otherwise
  createuser     Create a user, or change one with --force-update
      --email <EMAIL>     required
      --password <PASS>   read from stdin when left out
      --superuser         make the user an owner
      --force-update      if the user exists, set the password (and --superuser);
                          this ends their sessions

Options:
  -h, --help     Print this help
  -V, --version  Print the version";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Serve,
    Help,
    Version,
    CheckConfig,
    Migrate,
    Healthcheck,
    CreateUser(CreateUser),
}

#[derive(Debug, PartialEq, Eq)]
pub struct CreateUser {
    pub email: String,
    pub password: Option<String>,
    pub superuser: bool,
    pub force_update: bool,
}

/// Parses the arguments after the program name. The error names what was
/// wrong.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(Command::Serve);
    };
    let simple = match command.as_str() {
        "-h" | "--help" | "help" => Command::Help,
        "-V" | "--version" => Command::Version,
        "check-config" => Command::CheckConfig,
        "migrate" => Command::Migrate,
        "healthcheck" => Command::Healthcheck,
        "createuser" => return parse_createuser(args).map(Command::CreateUser),
        other => return Err(format!("unknown command '{other}'")),
    };
    match args.next() {
        None => Ok(simple),
        Some(extra) => Err(format!("unexpected argument '{extra}' for '{command}'")),
    }
}

fn parse_createuser(mut args: impl Iterator<Item = String>) -> Result<CreateUser, String> {
    let (mut email, mut password, mut superuser, mut force_update) = (None, None, false, false);
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
            None => (arg, None),
        };
        let mut value = || {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--email" => email = Some(value()?),
            "--password" => password = Some(value()?),
            "--superuser" => superuser = true,
            "--force-update" => force_update = true,
            _ => return Err(format!("unexpected argument '{flag}' for 'createuser'")),
        }
    }
    Ok(CreateUser {
        email: email.ok_or("createuser needs --email")?,
        password,
        superuser,
        force_update,
    })
}

/// Creates the user, or with `force_update` changes an existing one: a new
/// password, which ends their sessions, and owner if `superuser`. It never
/// demotes, so resetting an owner's password keeps them one.
pub async fn create_user(pool: &DbPool, spec: &CreateUser, password: &str) -> AppResult<()> {
    if password.is_empty() {
        return Err(AppError::Validation("the password is empty".to_string()));
    }
    let role = if spec.superuser {
        UserRole::Admin
    } else {
        UserRole::Member
    };
    let Some(user) = UsersService::get_by_email(pool, &spec.email).await? else {
        let req = CreateUserRequest {
            email: spec.email.clone(),
            password: password.to_string(),
        };
        UsersService::create_user(pool, &req, role).await?;
        return Ok(());
    };
    if !spec.force_update {
        return Err(AppError::Validation(format!(
            "{} already exists; pass --force-update to change it",
            user.email
        )));
    }
    let hash = User::hash_password(password)?;
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(hash)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    if spec.superuser {
        sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(UserRole::Admin.as_str())
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
    }
    AuthSessionService::revoke_user_sessions(&mut *tx, user.id).await?;
    tx.commit().await?;
    Ok(())
}

/// Reads the password from stdin: one line, typed without echo on a
/// terminal, or piped (`printf %s "$PASS" | rustrak createuser ...`).
pub fn read_password() -> std::io::Result<String> {
    use std::io::IsTerminal;
    let stdin = std::io::stdin();
    let mut line = String::new();
    if !stdin.is_terminal() {
        stdin.read_line(&mut line)?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_string());
    }
    eprint!("Password: ");
    std::io::stderr().flush()?;
    #[cfg(unix)]
    let restore = {
        use nix::sys::termios::{tcgetattr, tcsetattr, LocalFlags, SetArg};
        let original = tcgetattr(&stdin)?;
        let mut silent = original.clone();
        silent.local_flags.remove(LocalFlags::ECHO);
        tcsetattr(&stdin, SetArg::TCSANOW, &silent)?;
        move || tcsetattr(std::io::stdin(), SetArg::TCSANOW, &original)
    };
    let read = stdin.read_line(&mut line);
    #[cfg(unix)]
    restore()?;
    eprintln!();
    read?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// Where the local server listens, from `HOST` and `PORT` as the server reads
/// them. A wildcard bind is reached on loopback.
pub fn local_address() -> Result<SocketAddr, String> {
    let defaults = crate::config::Config::default();
    let port = match std::env::var("PORT") {
        Ok(p) if !p.trim().is_empty() => p
            .trim()
            .parse()
            .map_err(|_| format!("PORT must be a valid number, got \"{p}\""))?,
        _ => defaults.port,
    };
    let host = std::env::var("HOST").unwrap_or(defaults.host);
    let host = match host.trim() {
        "" | "0.0.0.0" => "127.0.0.1",
        "::" | "[::]" => "::1",
        other => other,
    };
    (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve HOST {host}: {e}"))?
        .next()
        .ok_or_else(|| format!("HOST {host} resolves to no address"))
}

/// Asks `addr` for `/health/ready`: healthy only on a 200, which the server
/// answers once its database does. Std only, so a distroless image needs
/// nothing else.
pub fn healthcheck(addr: SocketAddr) -> Result<(), String> {
    let timeout = Duration::from_secs(5);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| format!("cannot connect to {addr}: {e}"))?;
    let io = |e: std::io::Error| format!("{addr}: {e}");
    stream.set_read_timeout(Some(timeout)).map_err(io)?;
    stream.set_write_timeout(Some(timeout)).map_err(io)?;
    stream
        .write_all(b"GET /health/ready HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .map_err(io)?;
    let mut head = [0u8; 64];
    let n = stream.read(&mut head).map_err(io)?;
    let head = String::from_utf8_lossy(&head[..n]);
    let status = head.lines().next().unwrap_or_default();
    match status.split_whitespace().nth(1) {
        Some("200") => Ok(()),
        _ => Err(format!("{addr}/health/ready answered \"{status}\"")),
    }
}
