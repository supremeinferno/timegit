use clap::{Args, Parser, Subcommand};
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, ExitCode},
};

// ============================================================================
// CLI
// ============================================================================

#[derive(Parser, Debug)]
#[command(
    name = "timegit",
    version,
    about = "A Git wrapper for manipulating commit timestamps",
    long_about = r#"
timegit is a thin wrapper around Git which makes it easy to
create and rewrite commits at arbitrary points in time.

Git remains the version control system. timegit only handles
timestamp calculation and delegates the actual operation to Git.
"#
)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand, Debug)]
enum CommandKind {
    /// Create a Git commit at a specific time.
    New(NewArgs),

    /// Rewrite an existing Git commit's timestamp.
    Amend(AmendArgs),

    /// Manage named time pointers.
    #[command(subcommand)]
    Pointer(PointerCommand),
}

#[derive(Args, Debug)]
struct NewArgs {
    /// Absolute timestamp.
    ///
    /// Example:
    ///   2026-01-01T12:00:00Z
    #[arg(value_name = "DATETIME")]
    datetime: Option<String>,

    /// Use a named pointer as the base timestamp.
    #[arg(long, value_name = "POINTER")]
    pointer: Option<String>,

    /// Offset from the pointer.
    ///
    /// Examples:
    ///   +2d
    ///   -4h
    ///   +30min
    ///   +2m
    ///   +1y
    #[arg(value_name = "OFFSET")]
    offset: Option<String>,

    /// Commit message.
    #[arg(short = 'm', long)]
    message: String,
}

#[derive(Args, Debug)]
struct AmendArgs {
    /// Commit hash to rewrite.
    #[arg(value_name = "COMMIT")]
    commit: String,

    /// Absolute timestamp.
    #[arg(value_name = "DATETIME", conflicts_with = "pointer")]
    datetime: Option<String>,

    /// Use a named pointer as the base timestamp.
    #[arg(long, value_name = "POINTER", conflicts_with = "datetime")]
    pointer: Option<String>,

    /// Offset from the pointer.
    ///
    /// Must be supplied together with --pointer.
    #[arg(value_name = "OFFSET")]
    offset: Option<String>,

    /// Optional replacement commit message.
    #[arg(short = 'm', long)]
    message: Option<String>,
}

#[derive(Subcommand, Debug)]
enum PointerCommand {
    /// Create a new pointer.
    New(PointerNewArgs),

    /// Move an existing pointer.
    Move(PointerMoveArgs),

    /// Delete a pointer.
    Delete(PointerDeleteArgs),

    /// List all pointers.
    List,

    /// Show a pointer.
    Show(PointerShowArgs),
}

#[derive(Args, Debug)]
struct PointerNewArgs {
    /// Pointer name.
    #[arg(value_name = "NAME")]
    name: String,

    /// Timestamp for the pointer.
    #[arg(value_name = "DATETIME", conflicts_with = "from_commit")]
    datetime: Option<String>,

    /// Create the pointer from an existing Git commit.
    #[arg(long, value_name = "COMMIT", conflicts_with = "datetime")]
    from_commit: Option<String>,
}

#[derive(Args, Debug)]
struct PointerMoveArgs {
    /// Pointer name.
    #[arg(value_name = "NAME")]
    name: String,

    /// Offset to move the pointer by.
    #[arg(value_name = "OFFSET")]
    offset: String,
}

#[derive(Args, Debug)]
struct PointerDeleteArgs {
    /// Pointer name.
    #[arg(value_name = "NAME")]
    name: String,
}

#[derive(Args, Debug)]
struct PointerShowArgs {
    /// Pointer name.
    #[arg(value_name = "NAME")]
    name: String,
}

// ============================================================================
// Errors
// ============================================================================

fn die(message: impl AsRef<str>) -> ! {
    eprintln!("timegit: {}", message.as_ref());
    std::process::exit(1);
}

// ============================================================================
// Git helpers
// ============================================================================

fn git_command() -> Command {
    Command::new("git")
}

fn git_output<I, S>(args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = git_command()
        .args(args)
        .output()
        .map_err(|e| format!("failed to execute git: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        return Err(if stderr.trim().is_empty() {
            format!(
                "git exited with status {}",
                output.status.code().unwrap_or(-1)
            )
        } else {
            stderr.trim().to_string()
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git<I, S>(args: I, envs: &[(&str, &str)]) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = git_command();

    command.args(args);

    for (key, value) in envs {
        command.env(key, value);
    }

    let status = command
        .status()
        .map_err(|e| format!("failed to execute git: {e}"))?;

    if !status.success() {
        return Err(format!(
            "git exited with status {}",
            status.code().unwrap_or(-1)
        ));
    }

    Ok(())
}

fn ensure_git_repo() {
    if let Err(error) = git_output(["rev-parse", "--git-dir"]) {
        die(format!("not a git repository: {error}"));
    }
}

fn commit_date(commit: &str) -> Result<String, String> {
    git_output(["show", "-s", "--format=%cI", commit])
}

fn verify_commit(commit: &str) {
    if let Err(error) = git_output(["rev-parse", "--verify", &format!("{commit}^{{commit}}")]) {
        die(format!("invalid commit '{commit}': {error}"));
    }
}

// ============================================================================
// Pointer storage
// ============================================================================
//
// Pointer files contain either:
//
//     2026-01-01T12:00:00Z
//
// or:
//
//     commit:<hash>
//
// This metadata belongs to timegit.
// The actual repository remains entirely owned by Git.
// ============================================================================

fn timegit_home() -> PathBuf {
    if let Ok(value) = env::var("TIMEGIT_HOME") {
        return PathBuf::from(value);
    }

    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| die("could not determine home directory"));

    PathBuf::from(home).join(".timegit")
}

fn pointers_dir() -> PathBuf {
    timegit_home().join("pointers")
}

fn pointer_path(name: &str) -> PathBuf {
    pointers_dir().join(name)
}

fn validate_pointer_name(name: &str) {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        die(format!("invalid pointer name '{name}'"));
    }
}

fn pointer_exists(name: &str) -> bool {
    pointer_path(name).exists()
}

fn save_pointer(name: &str, value: &str) {
    validate_pointer_name(name);

    fs::create_dir_all(pointers_dir())
        .unwrap_or_else(|e| die(format!("cannot create pointer directory: {e}")));

    fs::write(pointer_path(name), value)
        .unwrap_or_else(|e| die(format!("cannot save pointer '{name}': {e}")));
}

fn load_pointer(name: &str) -> String {
    validate_pointer_name(name);

    fs::read_to_string(pointer_path(name))
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| die(format!("pointer '{name}' does not exist")))
}

fn delete_pointer(name: &str) {
    validate_pointer_name(name);

    let path = pointer_path(name);

    if !path.exists() {
        die(format!("pointer '{name}' does not exist"));
    }

    fs::remove_file(path).unwrap_or_else(|e| die(format!("cannot delete pointer '{name}': {e}")));
}

// ============================================================================
// Pointer resolution
// ============================================================================

fn resolve_pointer(name: &str) -> Result<String, String> {
    let value = load_pointer(name);

    if let Some(commit) = value.strip_prefix("commit:") {
        return commit_date(commit);
    }

    Ok(value)
}

fn list_pointers() {
    let directory = pointers_dir();

    if !directory.exists() {
        return;
    }

    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|e| die(format!("cannot read pointer directory: {e}")))
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();

        let value = match fs::read_to_string(entry.path()) {
            Ok(value) => value.trim().to_string(),
            Err(_) => continue,
        };

        if let Some(commit) = value.strip_prefix("commit:") {
            match commit_date(commit) {
                Ok(date) => {
                    println!("{:<20} {}", name, date);
                }
                Err(_) => {
                    println!("{:<20} <invalid commit: {}>", name, commit);
                }
            }
        } else {
            println!("{:<20} {}", name, value);
        }
    }
}

// ============================================================================
// Date/time
// ============================================================================

#[derive(Debug, Clone, Copy)]
enum OffsetUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Months,
    Years,
}

#[derive(Debug, Clone, Copy)]
struct Offset {
    sign: i64,
    amount: i64,
    unit: OffsetUnit,
}

fn parse_offset(value: &str) -> Result<Offset, String> {
    if value.len() < 3 {
        return Err(format!("invalid offset '{value}'"));
    }

    let sign = match value.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return Err(format!("offset must begin with '+' or '-': '{value}'")),
    };

    let rest = &value[1..];

    let split = rest
        .find(|character: char| !character.is_ascii_digit())
        .ok_or_else(|| format!("missing offset unit in '{value}'"))?;

    if split == 0 {
        return Err(format!("missing offset amount in '{value}'"));
    }

    let amount = rest[..split]
        .parse::<i64>()
        .map_err(|_| format!("invalid offset amount in '{value}'"))?;

    let unit = match &rest[split..] {
        "s" => OffsetUnit::Seconds,
        "min" => OffsetUnit::Minutes,
        "h" => OffsetUnit::Hours,
        "d" => OffsetUnit::Days,
        "m" => OffsetUnit::Months,
        "y" => OffsetUnit::Years,
        unit => {
            return Err(format!(
                "unknown offset unit '{unit}' in '{value}' \
                 (use s, min, h, d, m, y)"
            ));
        }
    };

    Ok(Offset { sign, amount, unit })
}

#[derive(Debug, Clone, Copy)]
struct DateTime {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    offset_seconds: i64,
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,

        4 | 6 | 9 | 11 => 30,

        2 if is_leap_year(year) => 29,

        2 => 28,

        _ => 0,
    }
}

fn parse_datetime(value: &str) -> Result<DateTime, String> {
    let value = value.trim();

    if value.is_empty() {
        return Err("datetime cannot be empty".into());
    }

    let (main, offset_seconds) = if value.ends_with('Z') || value.ends_with('z') {
        (&value[..value.len() - 1], 0)
    } else {
        let position = value
            .char_indices()
            .skip(10)
            .filter(|(_, c)| *c == '+' || *c == '-')
            .map(|(i, _)| i)
            .last()
            .ok_or_else(|| {
                format!(
                    "datetime must contain a timezone \
                         (for example 2026-01-01T12:00:00Z)"
                )
            })?;

        let main = &value[..position];
        let timezone = &value[position..];

        let timezone = timezone.replace(':', "");

        if timezone.len() != 5 {
            return Err(format!("invalid timezone in '{value}'"));
        }

        let sign = if timezone.starts_with('+') { 1 } else { -1 };

        let hours = timezone[1..3]
            .parse::<i64>()
            .map_err(|_| format!("invalid timezone in '{value}'"))?;

        let minutes = timezone[3..5]
            .parse::<i64>()
            .map_err(|_| format!("invalid timezone in '{value}'"))?;

        if hours > 23 || minutes > 59 {
            return Err(format!("invalid timezone in '{value}'"));
        }

        (main, sign * (hours * 3600 + minutes * 60))
    };

    let main = main.replace('T', " ");

    let mut parts = main.split_whitespace();

    let date = parts
        .next()
        .ok_or_else(|| format!("invalid datetime '{value}'"))?;

    let time = parts.next().unwrap_or("00:00:00");

    let mut date_parts = date.split('-');

    let year = date_parts
        .next()
        .ok_or_else(|| format!("invalid date '{value}'"))?
        .parse::<i64>()
        .map_err(|_| format!("invalid year in '{value}'"))?;

    let month = date_parts
        .next()
        .ok_or_else(|| format!("invalid date '{value}'"))?
        .parse::<i64>()
        .map_err(|_| format!("invalid month in '{value}'"))?;

    let day = date_parts
        .next()
        .ok_or_else(|| format!("invalid date '{value}'"))?
        .parse::<i64>()
        .map_err(|_| format!("invalid day in '{value}'"))?;

    if !(1..=12).contains(&month) {
        return Err(format!("invalid month in '{value}'"));
    }

    let max_day = days_in_month(year, month);

    if day < 1 || day > max_day {
        return Err(format!("invalid day in '{value}'"));
    }

    let mut time_parts = time.split(':');

    let hour = time_parts
        .next()
        .unwrap_or("0")
        .parse::<i64>()
        .map_err(|_| format!("invalid hour in '{value}'"))?;

    let minute = time_parts
        .next()
        .unwrap_or("0")
        .parse::<i64>()
        .map_err(|_| format!("invalid minute in '{value}'"))?;

    let second = time_parts
        .next()
        .unwrap_or("0")
        .split('.')
        .next()
        .unwrap_or("0")
        .parse::<i64>()
        .map_err(|_| format!("invalid second in '{value}'"))?;

    if hour > 23 || minute > 59 || second > 59 {
        return Err(format!("invalid time in '{value}'"));
    }

    Ok(DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        offset_seconds,
    })
}

// Howard Hinnant's civil calendar conversion.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };

    let era = if year >= 0 { year } else { year - 399 } / 400;

    let year_of_era = year - era * 400;

    let month_prime = month + if month > 2 { -3 } else { 9 };

    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;

    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146097 + day_of_era - 719468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719468;

    let era = if days >= 0 { days } else { days - 146096 } / 146097;

    let day_of_era = days - era * 146097;

    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;

    let year = year_of_era + era * 400;

    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);

    let month_prime = (5 * day_of_year + 2) / 153;

    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;

    let month = month_prime + if month_prime < 10 { 3 } else { -9 };

    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

fn format_datetime(datetime: DateTime) -> String {
    let sign = if datetime.offset_seconds >= 0 {
        '+'
    } else {
        '-'
    };

    let offset = datetime.offset_seconds.abs();

    let hours = offset / 3600;
    let minutes = (offset % 3600) / 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
        datetime.year,
        datetime.month,
        datetime.day,
        datetime.hour,
        datetime.minute,
        datetime.second,
        sign,
        hours,
        minutes
    )
}

fn add_offset(datetime: &str, offset: Offset) -> Result<String, String> {
    let mut datetime = parse_datetime(datetime)?;

    let amount = offset.sign * offset.amount;

    match offset.unit {
        OffsetUnit::Months => {
            let total_months = datetime.year * 12 + datetime.month - 1 + amount;

            let year = total_months.div_euclid(12);
            let month = total_months.rem_euclid(12) + 1;

            let max_day = days_in_month(year, month);

            if datetime.day > max_day {
                return Err(format!(
                    "{:04}-{:02}-{:02} cannot be shifted by {} months",
                    datetime.year, datetime.month, datetime.day, amount
                ));
            }

            datetime.year = year;
            datetime.month = month;
        }

        OffsetUnit::Years => {
            let year = datetime.year + amount;

            let max_day = days_in_month(year, datetime.month);

            if datetime.day > max_day {
                return Err(format!(
                    "{:04}-{:02}-{:02} cannot be shifted by {} years",
                    datetime.year, datetime.month, datetime.day, amount
                ));
            }

            datetime.year = year;
        }

        OffsetUnit::Seconds | OffsetUnit::Minutes | OffsetUnit::Hours | OffsetUnit::Days => {
            let seconds = match offset.unit {
                OffsetUnit::Seconds => amount,
                OffsetUnit::Minutes => amount * 60,
                OffsetUnit::Hours => amount * 60 * 60,
                OffsetUnit::Days => amount * 24 * 60 * 60,
                _ => unreachable!(),
            };

            let days = days_from_civil(datetime.year, datetime.month, datetime.day);

            let current_seconds =
                days * 86_400 + datetime.hour * 3_600 + datetime.minute * 60 + datetime.second;

            let new_seconds = current_seconds + seconds;

            let new_days = new_seconds.div_euclid(86_400);

            let seconds_of_day = new_seconds.rem_euclid(86_400);

            let (year, month, day) = civil_from_days(new_days);

            datetime.year = year;
            datetime.month = month;
            datetime.day = day;

            datetime.hour = seconds_of_day / 3_600;

            datetime.minute = (seconds_of_day % 3_600) / 60;

            datetime.second = seconds_of_day % 60;
        }
    }

    Ok(format_datetime(datetime))
}

// ============================================================================
// Commit creation
// ============================================================================

fn create_commit(datetime: &str, message: &str) {
    ensure_git_repo();

    // Parse ourselves so invalid dates are caught before Git is invoked.
    parse_datetime(datetime).unwrap_or_else(|e| die(format!("invalid datetime: {e}")));

    let result = run_git(
        ["commit", "-m", message],
        &[
            ("GIT_AUTHOR_DATE", datetime),
            ("GIT_COMMITTER_DATE", datetime),
        ],
    );

    if let Err(error) = result {
        die(error);
    }
}

fn command_new(args: NewArgs) {
    let (datetime, offset) = match (&args.datetime, &args.pointer, &args.offset) {
        (Some(datetime), None, None) => (datetime.clone(), None),

        (None, Some(pointer), Some(offset)) => {
            let base = resolve_pointer(pointer)
                .unwrap_or_else(|e| die(format!("cannot resolve pointer '{pointer}': {e}")));

            let offset = parse_offset(offset).unwrap_or_else(|e| die(e));

            let datetime = add_offset(&base, offset).unwrap_or_else(|e| die(e));

            (datetime, Some(offset))
        }

        (None, Some(_), None) => {
            die("--pointer requires an offset \
                     (example: --pointer start +2d)");
        }

        (None, None, Some(_)) => {
            die("an offset requires --pointer");
        }

        (Some(_), Some(_), _) => {
            die("cannot use DATETIME together with --pointer");
        }

        (Some(_), None, Some(_)) => {
            die("cannot use OFFSET without --pointer");
        }

        (None, None, None) => {
            die("provide DATETIME or \
                     --pointer POINTER OFFSET");
        }
    };

    let _ = offset;

    create_commit(&datetime, &args.message);
}

// ============================================================================
// Amend
// ============================================================================
//
// Important:
//
// Git commits are immutable. Changing a historical commit necessarily
// rewrites history.
//
// We deliberately let Git perform the rewrite using filter-branch rather
// than maintaining our own commit graph/database.
//
// timegit therefore remains a Git wrapper.
// ============================================================================

fn shell_quote(value: &str) -> String {
    // Single-quote for POSIX shell.
    // ' -> '\''.
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn amend_commit(commit: &str, datetime: &str, message: Option<&str>) {
    ensure_git_repo();
    verify_commit(commit);

    parse_datetime(datetime).unwrap_or_else(|e| die(format!("invalid datetime: {e}")));

    //
    // filter-branch exposes:
    //
    //   $GIT_COMMIT
    //   $GIT_AUTHOR_DATE
    //   $GIT_COMMITTER_DATE
    //
    // We only modify the requested commit.
    //
    // For message rewriting, msg-filter is used.
    //

    let commit_quoted = shell_quote(commit);
    let datetime_quoted = shell_quote(datetime);

    let env_filter = format!(
        r#"
if [ "$GIT_COMMIT" = {commit} ]; then
    export GIT_AUTHOR_DATE={date}
    export GIT_COMMITTER_DATE={date}
fi
"#,
        commit = commit_quoted,
        date = datetime_quoted,
    );

    let mut command = Command::new("git");

    command.args(["filter-branch", "-f", "--env-filter", &env_filter]);

    if let Some(message) = message {
        let message_quoted = shell_quote(message);

        let msg_filter = format!(
            r#"
if [ "$GIT_COMMIT" = {commit} ]; then
    printf '%s\n' {message}
else
    cat
fi
"#,
            commit = commit_quoted,
            message = message_quoted,
        );

        command.args(["--msg-filter", &msg_filter]);
    }

    command.arg("--");

    let status = command
        .status()
        .unwrap_or_else(|e| die(format!("failed to execute git filter-branch: {e}")));

    if !status.success() {
        die(format!(
            "git filter-branch exited with status {}",
            status.code().unwrap_or(-1)
        ));
    }

    println!("rewrote commit {} with timestamp {}", commit, datetime);
}

fn command_amend(args: AmendArgs) {
    ensure_git_repo();
    verify_commit(&args.commit);

    let datetime = match (&args.datetime, &args.pointer, &args.offset) {
        (Some(datetime), None, None) => datetime.clone(),

        (None, Some(pointer), Some(offset)) => {
            let base = resolve_pointer(pointer)
                .unwrap_or_else(|e| die(format!("cannot resolve pointer '{pointer}': {e}")));

            let offset = parse_offset(offset).unwrap_or_else(|e| die(e));

            add_offset(&base, offset).unwrap_or_else(|e| die(e))
        }

        (None, Some(_), None) => {
            die("--pointer requires an offset \
                 (example: --pointer start +2m)");
        }

        (None, None, Some(_)) => {
            die("an offset requires --pointer");
        }

        (Some(_), Some(_), _) => {
            die("cannot use DATETIME together with \
                 --pointer");
        }

        (Some(_), None, Some(_)) => {
            die("cannot use OFFSET without --pointer");
        }

        (None, None, None) => {
            die("provide DATETIME or \
                 --pointer POINTER OFFSET");
        }
    };

    amend_commit(&args.commit, &datetime, args.message.as_deref());
}

// ============================================================================
// Pointer commands
// ============================================================================

fn command_pointer_new(args: PointerNewArgs) {
    validate_pointer_name(&args.name);

    if pointer_exists(&args.name) {
        die(format!("pointer '{}' already exists", args.name));
    }

    match (args.datetime, args.from_commit) {
        (Some(datetime), None) => {
            parse_datetime(&datetime).unwrap_or_else(|e| die(format!("invalid datetime: {e}")));

            save_pointer(&args.name, &datetime);

            println!("created pointer '{}' -> {}", args.name, datetime);
        }

        (None, Some(commit)) => {
            ensure_git_repo();
            verify_commit(&commit);

            save_pointer(&args.name, &format!("commit:{commit}"));

            let date = commit_date(&commit).unwrap_or_else(|e| die(e));

            println!("created pointer '{}' -> {} ({})", args.name, commit, date);
        }

        (None, None) => {
            die("provide DATETIME or \
                 --from-commit COMMIT");
        }

        (Some(_), Some(_)) => {
            unreachable!("clap prevents this");
        }
    }
}

fn command_pointer_move(args: PointerMoveArgs) {
    let current = resolve_pointer(&args.name)
        .unwrap_or_else(|e| die(format!("cannot resolve pointer '{}': {}", args.name, e)));

    let offset = parse_offset(&args.offset).unwrap_or_else(|e| die(e));

    let new_value = add_offset(&current, offset).unwrap_or_else(|e| die(e));

    save_pointer(&args.name, &new_value);

    println!("{} -> {}", args.name, new_value);
}

fn command_pointer_delete(args: PointerDeleteArgs) {
    delete_pointer(&args.name);

    println!("deleted pointer '{}'", args.name);
}

fn command_pointer_show(args: PointerShowArgs) {
    let raw = load_pointer(&args.name);

    if let Some(commit) = raw.strip_prefix("commit:") {
        match commit_date(commit) {
            Ok(date) => {
                println!("name:   {}", args.name);
                println!("type:   commit");
                println!("commit: {}", commit);
                println!("date:   {}", date);
            }

            Err(error) => {
                println!("name:   {}", args.name);
                println!("type:   commit");
                println!("commit: {}", commit);
                println!("date:   <invalid>");
                println!("error:  {}", error);
            }
        }
    } else {
        println!("name: {}", args.name);
        println!("type: datetime");
        println!("date: {}", raw);
    }
}

// ============================================================================
// Dispatch
// ============================================================================

fn command_pointer(command: PointerCommand) {
    match command {
        PointerCommand::New(args) => {
            command_pointer_new(args);
        }

        PointerCommand::Move(args) => {
            command_pointer_move(args);
        }

        PointerCommand::Delete(args) => {
            command_pointer_delete(args);
        }

        PointerCommand::List => {
            list_pointers();
        }

        PointerCommand::Show(args) => {
            command_pointer_show(args);
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        CommandKind::New(args) => {
            command_new(args);
        }

        CommandKind::Amend(args) => {
            command_amend(args);
        }

        CommandKind::Pointer(command) => {
            command_pointer(command);
        }
    }

    ExitCode::SUCCESS
}
