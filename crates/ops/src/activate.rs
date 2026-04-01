use types::{Candidate, Identifier};

use crate::Error;

/// Returns shell commands (suitable for `eval`) that activate a specific version
/// of a candidate in the current shell session.
///
/// Equivalent to SDKMAN's `sdk use <candidate> <version>`:
/// - Sets `<CANDIDATE_UPPER>_HOME` to the specific version path (not `current`)
/// - Replaces the candidate's entry in `PATH` in-place if already present,
///   or prepends `<version>/bin` otherwise
///
/// Does NOT update the `current` symlink — that is `sdk default` / `ops::set_default`.
///
/// Reads `SDKMAN_DIR` and `PATH` from the environment and computes the new
/// values in Rust, emitting plain `export VAR="value"` assignments.
pub fn shell_activation_commands(
    candidate: &Candidate,
    identifier: &Identifier,
) -> Result<Vec<String>, Error> {
    let sdkman_dir = store::sdkman_dir()?;
    let sdkman_dir = sdkman_dir.to_string_lossy();

    let cand = candidate.as_str();
    let ver = identifier.as_str();

    let new_home = format!("{sdkman_dir}/candidates/{cand}/{ver}");
    let new_bin = format!("{new_home}/bin");
    let home_var = cand.to_uppercase() + "_HOME";

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = updated_path(&current_path, &sdkman_dir, cand, &new_bin);

    Ok(vec![
        format!("export {home_var}=\"{new_home}\""),
        format!("export PATH=\"{new_path}\""),
    ])
}

/// Replace the first PATH entry belonging to `candidate` with `new_bin`.
/// If none is found, prepend `new_bin`.
fn updated_path(current_path: &str, sdkman_dir: &str, candidate: &str, new_bin: &str) -> String {
    let prefix = format!("{sdkman_dir}/candidates/{candidate}/");
    let mut replaced = false;
    let entries: Vec<&str> = current_path
        .split(':')
        .map(|entry| {
            if !replaced && entry.starts_with(&prefix) {
                replaced = true;
                new_bin
            } else {
                entry
            }
        })
        .collect();

    if replaced {
        entries.join(":")
    } else {
        format!("{new_bin}:{current_path}")
    }
}
