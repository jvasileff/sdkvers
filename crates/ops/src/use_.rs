use types::{Candidate, Identifier};

use crate::{Error, activate::shell_activation_commands};

/// Activate a specific version of a candidate in the current shell session.
/// The version must already be installed.
///
/// Returns shell commands suitable for `eval` that update `<CANDIDATE>_HOME`
/// and replace the candidate's entry in `PATH`. Does not touch the `current`
/// symlink — use `set_default` for persistent changes.
pub fn use_version(candidate: &Candidate, identifier: &Identifier) -> Result<Vec<String>, Error> {
    if !store::version_path(candidate, identifier)?.exists() {
        return Err(store::Error::VersionNotInstalled {
            candidate: candidate.to_string(),
            identifier: identifier.to_string(),
        }.into());
    }
    shell_activation_commands(candidate, identifier)
}
