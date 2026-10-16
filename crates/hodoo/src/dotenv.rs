//! A `.env` file, for the credentials a shell does not export.
//!
//! The environment always wins: a value set in the process environment (and
//! therefore a command-line flag, or the shell's `export`) is never overridden
//! by a file. Below it, a project's `.env` wins over the per-user file
//! ([`user_file`]), which is what makes an installed binary work from any
//! directory. [`resolve`] and [`load_layered`] are the layering, in one place.
//!
//! Nothing here mutates the process environment: `std::env::set_var` is unsafe
//! in edition 2024 and this crate forbids unsafe code, so a file is read into a
//! map and consulted explicitly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How far up from the starting directory to look for a `.env`.
const MAX_DEPTH: usize = 4;

/// The variables this crate knows about.
pub const KEYS: [&str; 3] = ["ODOO_URL", "ODOO_API_KEY", "ODOO_DB"];

/// The value for `key`: the process environment first, then the file.
#[must_use]
pub fn resolve(key: &str, file: &BTreeMap<String, String>) -> Option<String> {
    std::env::var(key).ok().or_else(|| file.get(key).cloned())
}

/// Parses `.env` text.
///
/// Understands `KEY=value`, an optional `export ` prefix, `#` comments, blank
/// lines, and a value wrapped in single or double quotes. Anything else is
/// ignored rather than guessed at: a malformed line is not worth failing a
/// command over.
#[must_use]
pub fn parse(text: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = assignment(line) {
            values.insert(key.to_owned(), unquote(value.trim()));
        }
    }
    values
}

/// The `KEY=value` a line assigns, or `None` for a comment or a line that
/// assigns nothing.
///
/// One definition of what an assignment looks like, shared by [`parse`] and
/// [`set`], so writing a file cannot produce a line the reader would skip.
fn assignment(line: &str) -> Option<(&str, &str)> {
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    (!key.is_empty() && !key.contains(char::is_whitespace)).then_some((key, value))
}

/// Reads a `.env` file.
///
/// # Errors
///
/// Any [`std::io::Error`] from reading the file.
pub fn read(path: impl AsRef<Path>) -> std::io::Result<BTreeMap<String, String>> {
    std::fs::read_to_string(path).map(|text| parse(&text))
}

/// The nearest `.env` at or above `start`, up to `MAX_DEPTH` levels.
#[must_use]
pub fn find(start: impl AsRef<Path>) -> Option<PathBuf> {
    let start = start.as_ref();
    let mut directory = Some(start);
    for _ in 0..=MAX_DEPTH {
        let current = directory?;
        let candidate = current.join(".env");
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = current.parent();
    }
    None
}

/// Reads the nearest `.env`, if there is one.
///
/// # Errors
///
/// Any [`std::io::Error`] from reading a file that was found.
pub fn load(start: impl AsRef<Path>) -> std::io::Result<Option<BTreeMap<String, String>>> {
    match find(start) {
        Some(path) => read(path).map(Some),
        None => Ok(None),
    }
}

/// The per-user file, `$XDG_CONFIG_HOME/hodoo/env` or `~/.config/hodoo/env`,
/// whether or not it exists.
///
/// Same `KEY=value` format as a `.env`. It is what an installed `hodoo` reads when
/// the working directory has no `.env` above it, like `gh` or `aws` reading their
/// files under the home directory.
#[must_use]
pub fn user_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("hodoo").join("env"))
}

/// Sets `key` to `value` in a `.env` file, creating the file and its directory
/// if either is missing.
///
/// Every other line is kept as it was, comments included, because the same file
/// may hold credentials this function knows nothing about. The result is chmod
/// 600 on Unix: the file holds an API key, the same file that gets a warning
/// when someone else can read it.
///
/// # Errors
///
/// Any [`std::io::Error`] from creating the directory or writing the file.
pub fn set(path: impl AsRef<Path>, key: &str, value: &str) -> std::io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let mut kept: Vec<&str> = existing
        .lines()
        .filter(|line| assignment(line.trim()).map(|(found, _)| found) != Some(key))
        .collect();
    // The assignment goes last: a bare value the reader trims stays as written.
    let written = format!("{key}={}", quote(value));
    kept.push(&written);
    let text = format!("{}\n", kept.join("\n"));

    std::fs::write(path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Quotes a value that a bare write would not read back unchanged: one with
/// whitespace at either end, or one that itself starts and ends with a quote,
/// which [`unquote`] would strip.
fn quote(value: &str) -> String {
    let wrapped = |quote: char| value.starts_with(quote) && value.ends_with(quote);
    let bare_reads_back = value.trim() == value && !wrapped('"') && !wrapped('\'');
    if bare_reads_back {
        return value.to_owned();
    }
    let delimiter = if value.contains('\'') { '"' } else { '\'' };
    format!("{delimiter}{value}{delimiter}")
}

/// The per-user file with the nearest `.env` at or above `start` laid over it.
///
/// # Errors
///
/// Any [`std::io::Error`] from reading a file that was found.
pub fn load_layered(start: impl AsRef<Path>) -> std::io::Result<BTreeMap<String, String>> {
    let mut values = match user_file().filter(|path| path.is_file()) {
        Some(path) => read(path)?,
        None => BTreeMap::new(),
    };
    values.extend(load(start)?.unwrap_or_default());
    Ok(values)
}

/// Whether a credentials file can be read by anyone but its owner.
///
/// The file holds an API key, so group- or world-readable is worth a warning,
/// as `ssh` gives for a private key. Always `false` off Unix, where the mode
/// bits do not exist.
#[must_use]
pub fn readable_by_others(path: impl AsRef<Path>) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o077 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn unquote(value: &str) -> String {
    // Only a matching opening and closing quote is a delimiter; a value that
    // merely contains or starts with a quote keeps it, rather than losing a
    // character to a half-applied rule.
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        return value[1..value.len() - 1].to_owned();
    }
    value.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_yields_its_keys_and_ignores_the_rest() {
        let parsed = parse(
            "# a comment\n\
             \n\
             ODOO_URL=https://odoo.example.com\n\
             export ODOO_API_KEY=\"secret value\"\n\
             ODOO_DB='odoo'\n\
             not a variable\n\
             =novalue\n",
        );
        assert_eq!(
            parsed.get("ODOO_URL").map(String::as_str),
            Some("https://odoo.example.com")
        );
        assert_eq!(
            parsed.get("ODOO_API_KEY").map(String::as_str),
            Some("secret value")
        );
        assert_eq!(parsed.get("ODOO_DB").map(String::as_str), Some("odoo"));
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn the_process_environment_wins_over_the_file() {
        // `PATH` is always set, so it proves precedence without mutating the
        // environment (which needs `unsafe` in edition 2024, and this crate
        // forbids unsafe code).
        let file = BTreeMap::from([("PATH".to_owned(), "from the file".to_owned())]);
        let resolved = resolve("PATH", &file).expect("PATH");
        assert_eq!(Some(resolved), std::env::var("PATH").ok());

        // A key only the file carries is taken from the file.
        let file = BTreeMap::from([("HODOO_NOT_IN_ENV".to_owned(), "from the file".to_owned())]);
        assert_eq!(
            resolve("HODOO_NOT_IN_ENV", &file).as_deref(),
            Some("from the file")
        );
        assert_eq!(resolve("HODOO_NOT_ANYWHERE", &file), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_others_can_read_is_flagged() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("hodoo-perm-{}", std::process::id()));
        std::fs::write(&path, "ODOO_API_KEY=x\n").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        assert!(readable_by_others(&path));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
        assert!(!readable_by_others(&path));
        std::fs::remove_file(&path).expect("cleanup");
    }

    #[test]
    fn setting_a_key_creates_the_file_and_its_directory() {
        let root = std::env::temp_dir().join(format!("hodoo-set-{}", std::process::id()));
        let path = root.join("hodoo").join("env");
        assert!(!path.exists());

        set(&path, "ODOO_URL", "https://odoo.example.com").expect("set");

        let parsed = read(&path).expect("read");
        assert_eq!(
            parsed.get("ODOO_URL").map(String::as_str),
            Some("https://odoo.example.com")
        );
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn setting_a_key_again_replaces_it_and_keeps_the_rest() {
        let root = std::env::temp_dir().join(format!("hodoo-set-again-{}", std::process::id()));
        std::fs::create_dir_all(root.join("hodoo")).expect("temp dir");
        let path = root.join("hodoo").join("env");
        std::fs::write(
            &path,
            "# mine\nODOO_URL=https://old.example.com\nODOO_DB=odoo\n",
        )
        .expect("write");

        set(&path, "ODOO_URL", "https://new.example.com").expect("set");

        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            text,
            "# mine\nODOO_DB=odoo\nODOO_URL=https://new.example.com\n"
        );
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_value_that_needs_quoting_reads_back_unchanged() {
        let root = std::env::temp_dir().join(format!("hodoo-set-quote-{}", std::process::id()));
        let path = root.join("env");

        for value in [" spaced ", "'already quoted'", "a\tb", "plain-key-abc123"] {
            set(&path, "ODOO_API_KEY", value).expect("set");
            assert_eq!(
                read(&path)
                    .expect("read")
                    .get("ODOO_API_KEY")
                    .map(String::as_str),
                Some(value),
                "{value:?} did not survive the write"
            );
        }
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn a_written_file_is_readable_by_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("hodoo-set-mode-{}", std::process::id()));
        let path = root.join("env");
        std::fs::create_dir_all(&root).expect("temp dir");
        std::fs::write(&path, "ODOO_API_KEY=x\n").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");

        set(&path, "ODOO_API_KEY", "y").expect("set");

        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn the_nearest_file_upwards_is_the_one_found() {
        let root = std::env::temp_dir().join(format!("hodoo-dotenv-{}", std::process::id()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).expect("temp dirs");
        std::fs::write(root.join(".env"), "ODOO_URL=from-the-root\n").expect("write");

        let found = find(&nested).expect("found");
        assert_eq!(found, root.join(".env"));
        assert_eq!(
            read(&found)
                .expect("read")
                .get("ODOO_URL")
                .map(String::as_str),
            Some("from-the-root")
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
