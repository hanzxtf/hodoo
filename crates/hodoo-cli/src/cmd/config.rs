//! `hodoo config url|api-key`: the credentials an installed binary needs, written
//! where it looks for them.
//!
//! Nothing here talks to Odoo, which is the point: this is how the first key gets
//! onto a machine, so it cannot require one.

use serde_json::json;

use crate::Failure;
use crate::cli::{ConfigCmd, Global, text_or_stdin};
use crate::cmd::output_of;

/// Writes one credential to the per-user file.
///
/// # Errors
///
/// [`Failure::Usage`] for an empty key or a URL the client could not use, and
/// [`Failure::Io`] when the file cannot be written.
pub fn run(global: &Global, command: &ConfigCmd) -> Result<(), Failure> {
    let out = output_of(global)?;
    let (key, value) = match command {
        ConfigCmd::Url { url } => {
            // The client's own rule for a base URL, so a saved URL is never one
            // that fails the first time it is used.
            hodoo::Config::new(url, "")?;
            ("ODOO_URL", url.clone())
        }
        ConfigCmd::ApiKey { api_key } => {
            let secret = text_or_stdin(api_key).map_err(Failure::Usage)?;
            if secret.is_empty() {
                return Err(Failure::Usage(
                    "the API key is empty. Pass the key Odoo shows under Preferences > \
                     Account Security, or - to read it from stdin"
                        .to_owned(),
                ));
            }
            ("ODOO_API_KEY", secret)
        }
    };

    let Some(path) = hodoo::dotenv::user_file() else {
        return Err(Failure::Usage(
            "no home directory to write to. Set HOME or XDG_CONFIG_HOME, or export \
             ODOO_URL and ODOO_API_KEY in the shell instead"
                .to_owned(),
        ));
    };
    // Never the value: it is a secret, and JSON output is often a log.
    let result = json!({ "key": key, "ok": true, "path": path.display().to_string() });

    if global.dry_run {
        out.result(&json!({ "dry_run": true, "key": key, "path": path.display().to_string() }))?;
        return Ok(out.note(&format!("would save {key} to {}", path.display()))?);
    }

    hodoo::dotenv::set(&path, key, &value)?;
    out.result(&result)?;
    out.note(&format!("saved {key} to {}", path.display()))?;
    out.hint("hodoo whoami  ·  proves the URL, the certificate and the key in one go")?;
    Ok(())
}
