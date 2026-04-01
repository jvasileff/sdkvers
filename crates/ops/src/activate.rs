use types::{Candidate, Identifier};

/// Returns shell commands (suitable for `eval`) that activate a specific version
/// of a candidate in the current shell session.
///
/// Equivalent to SDKMAN's `sdk use <candidate> <version>`:
/// - Sets `<CANDIDATE_UPPER>_HOME` to the specific version path (not `current`)
/// - Replaces the candidate's entry in `PATH` in-place if already present,
///   or prepends `<version>/bin` otherwise
///
/// Does NOT update the `current` symlink — that is `sdk default` / `ops::set_default`.
pub fn shell_activation_commands(candidate: &Candidate, identifier: &Identifier) -> Vec<String> {
    let cand = candidate.as_str();
    let ver = identifier.as_str();
    let home_var = cand.to_uppercase() + "_HOME";

    vec![
        // Set <CANDIDATE>_HOME to the specific version, not the 'current' symlink.
        format!("export {home_var}=\"$SDKMAN_DIR/candidates/{cand}/{ver}\""),

        // Replace the candidate's path segment in PATH in-place.
        // Captures whatever version is currently in PATH (e.g. "current" or a prior
        // specific version) using a capture group, then substitutes.
        // Compatible with both bash (BASH_REMATCH) and zsh (match).
        // Falls back to prepending <version>/bin if the candidate isn't in PATH yet.
        format!(
            "if [[ \"$PATH\" =~ $SDKMAN_DIR/candidates/{cand}/([^:/]+) ]]; then \
             _sdkvers_v=\"${{BASH_REMATCH[1]:-${{match[1]}}}}\"; \
             export PATH=\"${{PATH//$SDKMAN_DIR/candidates/{cand}/$_sdkvers_v/$SDKMAN_DIR/candidates/{cand}/{ver}}}\"; \
             unset _sdkvers_v; \
             else \
             export PATH=\"$SDKMAN_DIR/candidates/{cand}/{ver}/bin:$PATH\"; \
             fi"
        ),
    ]
}
