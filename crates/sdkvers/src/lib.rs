use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub use types::{
    Atom, AtomKind, Component, ConfigLineNode, ConfigLineParser, DocumentLineNode, DocumentNode,
    Error, HookFingerprint, Identifier, ParsedIdentifier, Platform, Resolver, Result,
    SdkListNode, SdkListRow, ResolvedRow, Separator, SdkVersion, Vendor, VersionExprNode,
    VersionNode, VersionParser, ArchiveFormat, Candidate,
    dump_config_line, dump_document, dump_sdk_list, dump_version, dump_version_expr,
    parse_document, parse_sdk_list,
};

fn err(message: impl Into<String>) -> Error {
    Error(message.into())
}

pub fn load_local_sdk_list(candidate: &str) -> Result<SdkListNode> {
    let cand = Candidate::new(candidate);
    let installed = store::list_installed(&cand).map_err(|e| err(e.to_string()))?;
    let rows = installed.into_iter().map(|iv| {
        let status = if iv.is_current {
            Some("current local only".to_string())
        } else {
            Some("local only".to_string())
        };
        SdkListRow {
            candidate: candidate.to_string(),
            version: iv.version.to_string(),
            vendor_label: iv.vendor.as_ref().map(|v| v.to_string()),
            dist: iv.vendor.as_ref().map(|v| v.to_string()),
            status,
            identifier: Some(iv.identifier.to_string()),
            in_use: iv.is_current,
        }
    }).collect();
    Ok(SdkListNode {
        candidate: candidate.to_string(),
        rows,
    })
}

pub fn list_sdkman_candidates() -> Result<Vec<String>> {
    store::list_candidates()
        .map_err(|e| err(e.to_string()))
        .map(|v| v.into_iter().map(|c| c.to_string()).collect())
}

pub fn bootstrap_sdkvers_content() -> Result<String> {
    let candidates = list_sdkman_candidates()?;
    if candidates.is_empty() {
        return Err(err("no SDKMAN candidates found"));
    }
    let mut lines = Vec::new();
    for candidate in &candidates {
        let sdk = load_local_sdk_list(candidate)?;
        if let Some(row) = sdk.rows.iter().find(|r| r.in_use) {
            let line = match &row.dist {
                Some(dist) => format!("{} = {} {}", candidate, row.version, dist),
                None => format!("{} = {}", candidate, row.version),
            };
            lines.push(line);
        }
    }
    if lines.is_empty() {
        return Err(err("no active SDKMAN versions found"));
    }
    Ok(lines.join("\n") + "\n")
}


pub fn run_sdk_list(candidate: &str) -> Result<String> {
    let init_path = sdkman_init_path()?;
    let script = format!(
        ". '{}' >/dev/null 2>&1; sdk list '{}'",
        shell_escape_single_quoted(&init_path),
        shell_escape_single_quoted(candidate)
    );
    let output = Command::new("bash")
        .arg("-c")
        .arg(script)
        .output()
        .map_err(|e| err(format!("failed to run sdk list {candidate}: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(err(format!(
                "sdk list {candidate} failed with exit code {}",
                output.status.code().unwrap_or(1)
            )));
        }
        return Err(err(format!(
            "sdk list {candidate} failed with exit code {}: {stderr}",
            output.status.code().unwrap_or(1)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn sdkman_init_path() -> Result<String> {
    if let Ok(dir) = env::var("SDKMAN_DIR") {
        return Ok(format!("{dir}/bin/sdkman-init.sh"));
    }
    let home = env::var("HOME").map_err(|_| err("could not determine home directory for SDKMAN lookup"))?;
    Ok(format!("{home}/.sdkman/bin/sdkman-init.sh"))
}

// Escapes a string for use inside single quotes in a POSIX shell command.
// Single quotes cannot be escaped inside single quotes; the technique is to
// end the single-quoted string, insert a double-quoted single quote, then
// reopen the single-quoted string: ' → '"'"'
fn shell_escape_single_quoted(text: &str) -> String {
    text.replace('\'', "'\"'\"'")
}

pub fn read_utf8_file(path: &str) -> Result<String> {
    fs::read_to_string(path).map_err(|e| err(format!("failed to read file: {path}: {e}")))
}

pub fn find_sdkvers_path(start_path: &str) -> Result<String> {
    let requested = fs::canonicalize(start_path).unwrap_or_else(|_| PathBuf::from(start_path));
    let mut current = if requested.is_dir() {
        requested.clone()
    } else {
        requested
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(requested.clone())
    };
    loop {
        let candidate = current.join(".sdkvers");
        if candidate.is_file() {
            return Ok(candidate.to_string_lossy().to_string());
        }
        if !current.pop() {
            return Err(err(format!(
                "no .sdkvers file found from: {}",
                requested.to_string_lossy()
            )));
        }
    }
}

pub fn resolve_document(path: &str) -> Result<Vec<String>> {
    let (commands, errors) = resolve_document_with_details(path)?;
    if errors.is_empty() {
        Ok(commands)
    } else {
        Err(err(format!("{}\n{}", commands.join("\n"), errors.join("\n"))))
    }
}

pub fn suggest_install(line: &ConfigLineNode) -> Option<String> {
    let text = run_sdk_list(&line.candidate).ok()?;
    let sdk = parse_sdk_list(&line.candidate, &text);
    let row = find_best_uninstalled_for_suggestion(line, &sdk);
    match row.ok()? {
        Some(row) => {
            let id = row.identifier.unwrap_or(row.version);
            Some(format!("try: sdk install {} {}", line.candidate, id))
        }
        None => Some(format!(
            "{} {} is not available via SDKMAN; check: sdk list {}",
            line.candidate,
            line.expr.source(),
            line.candidate,
        )),
    }
}

// For java with no explicit vendor, prefer vendors the user already has installed
// so the suggestion stays within their established toolchain. Falls back to all
// vendors if no match exists within installed vendors.
fn find_best_uninstalled_for_suggestion(
    line: &ConfigLineNode,
    sdk: &SdkListNode,
) -> Result<Option<SdkListRow>> {
    if line.candidate == "java" && line.vendor.is_none() {
        let local = load_local_sdk_list("java").unwrap_or_else(|_| SdkListNode {
            candidate: "java".to_string(),
            rows: vec![],
        });
        let installed_dists: std::collections::HashSet<String> =
            local.rows.iter().filter_map(|r| r.dist.clone()).collect();
        if !installed_dists.is_empty() {
            let filtered = SdkListNode {
                candidate: sdk.candidate.clone(),
                rows: sdk
                    .rows
                    .iter()
                    .filter(|r| {
                        r.dist
                            .as_ref()
                            .map(|d| installed_dists.contains(d))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect(),
            };
            let result = Resolver.find_best_uninstalled(line, &filtered)?;
            if result.is_some() {
                return Ok(result);
            }
        }
    }
    Resolver.find_best_uninstalled(line, sdk)
}

pub fn resolve_document_with_details(path: &str) -> Result<(Vec<String>, Vec<String>)> {
    let document = parse_document(&read_utf8_file(path)?);
    let mut cache: HashMap<String, SdkListNode> = HashMap::new();
    let resolver = Resolver;
    let mut commands = Vec::new();
    let mut errors = Vec::new();
    for entry in document.entries {
        let config = match ConfigLineParser::new(&entry.source, entry.line_number).parse_line() {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("error: {} (line {} in {})", e.0, entry.line_number, path));
                continue;
            }
        };
        let sdk_list = match cache.get(&config.candidate) {
            Some(existing) => existing.clone(),
            None => match load_local_sdk_list(&config.candidate) {
                Ok(parsed) => {
                    cache.insert(config.candidate.clone(), parsed.clone());
                    parsed
                }
                Err(e) => {
                    errors.push(format!("error: {} (line {} in {})", e.0, entry.line_number, path));
                    continue;
                }
            },
        };
        match resolver.resolve_line(&config, &sdk_list) {
            Ok(row) => commands.push(format!("sdk use {} {}", row.candidate, row.target)),
            Err(e) => {
                let mut msg = format!("error: {} (line {} in {})", e.0, entry.line_number, path);
                if let Some(hint) = suggest_install(&config) {
                    msg.push_str(&format!("\nhint: {hint}"));
                }
                errors.push(msg);
            }
        }
    }
    Ok((commands, errors))
}
pub fn self_test(report: impl Fn(&str)) -> Result<()> {
    report("live sdk list");
    let live_sdk_text = run_sdk_list("java")?;
    if !live_sdk_text.contains("Available Java Versions") {
        return Err(err("self-test failed: live sdk list"));
    }
    let live_java_sdk = parse_sdk_list("java", &live_sdk_text);
    if live_java_sdk.rows.is_empty() {
        return Err(err("self-test failed: live java sdk parse"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::PathBuf};

    // ---- helpers ----

    fn parse_expr(s: &str) -> VersionExprNode {
        VersionParser::new(s)
            .parse_version_expr()
            .unwrap_or_else(|e| panic!("parse_expr({s:?}): {e}"))
    }

    fn make_line(s: &str) -> ConfigLineNode {
        ConfigLineParser::new(s, 1)
            .parse_line()
            .unwrap_or_else(|e| panic!("make_line({s:?}): {e}"))
    }

    fn sdk_list(candidate: &str, identifiers: &[&str]) -> SdkListNode {
        let rows = identifiers
            .iter()
            .map(|id| {
                let (version, dist) = if candidate == "java" {
                    match id.rfind('-') {
                        Some(idx) if idx > 0 => {
                            (id[..idx].to_string(), Some(id[idx + 1..].to_string()))
                        }
                        _ => (id.to_string(), None),
                    }
                } else {
                    (id.to_string(), None)
                };
                SdkListRow {
                    candidate: candidate.to_string(),
                    version,
                    vendor_label: None,
                    dist,
                    status: Some("local only".to_string()),
                    identifier: Some(id.to_string()),
                    in_use: false,
                }
            })
            .collect();
        SdkListNode {
            candidate: candidate.to_string(),
            rows,
        }
    }

    fn cmp(a: &str, b: &str) -> i32 {
        let r = Resolver;
        r.compare_versions(a, b)
            .unwrap_or_else(|e| panic!("cmp({a:?}, {b:?}): {e}"))
    }

    fn expr_matches(expr: &str, version: &str) -> bool {
        let r = Resolver;
        r.version_expr_matches(&parse_expr(expr), version)
            .unwrap_or_else(|e| panic!("expr_matches({expr:?}, {version:?}): {e}"))
    }

    // ---- parsing: bare version expansion ----

    #[test]
    fn bare_major_expands_to_major_range() {
        match parse_expr("21") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(lower_inclusive);
                assert!(!upper_inclusive);
                assert_eq!(lower.unwrap().source, "21");
                assert_eq!(upper.unwrap().source, "22");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn bare_minor_expands_to_minor_range() {
        match parse_expr("3.9") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(lower_inclusive);
                assert!(!upper_inclusive);
                assert_eq!(lower.unwrap().source, "3.9");
                assert_eq!(upper.unwrap().source, "3.10");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn bare_three_segment_is_exact() {
        assert!(matches!(parse_expr("8.7.0"), VersionExprNode::Exact { .. }));
    }

    #[test]
    fn bare_four_segment_is_exact() {
        assert!(matches!(parse_expr("4.10.1.3"), VersionExprNode::Exact { .. }));
    }

    #[test]
    fn bare_mixed_version_is_exact() {
        assert!(matches!(parse_expr("26.ea.35"), VersionExprNode::Exact { .. }));
    }

    #[test]
    fn bare_release_alias_version_is_exact() {
        assert!(matches!(
            parse_expr("2.16.0.Final"),
            VersionExprNode::Exact { .. }
        ));
    }

    #[test]
    fn bare_underscore_version_is_exact() {
        assert!(matches!(
            parse_expr("5.23.0.0_2"),
            VersionExprNode::Exact { .. }
        ));
    }

    // ---- parsing: explicit ranges ----

    #[test]
    fn explicit_range_inclusive_lower_exclusive_upper() {
        match parse_expr("[21,22)") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(lower_inclusive);
                assert!(!upper_inclusive);
                assert_eq!(lower.unwrap().source, "21");
                assert_eq!(upper.unwrap().source, "22");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn explicit_range_inclusive_both_bounds() {
        match parse_expr("[3.9,4]") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(lower_inclusive);
                assert!(upper_inclusive);
                assert_eq!(lower.unwrap().source, "3.9");
                assert_eq!(upper.unwrap().source, "4");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn explicit_range_exclusive_lower_inclusive_upper() {
        match parse_expr("(21,22]") {
            VersionExprNode::Range {
                lower_inclusive,
                lower,
                upper,
                upper_inclusive,
                ..
            } => {
                assert!(!lower_inclusive);
                assert!(upper_inclusive);
                assert_eq!(lower.unwrap().source, "21");
                assert_eq!(upper.unwrap().source, "22");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn explicit_range_no_upper_bound() {
        match parse_expr("[21,)") {
            VersionExprNode::Range { lower, upper, .. } => {
                assert!(lower.is_some());
                assert!(upper.is_none());
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn explicit_range_no_lower_bound() {
        match parse_expr("(,22)") {
            VersionExprNode::Range { lower, upper, .. } => {
                assert!(lower.is_none());
                assert!(upper.is_some());
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn bracketed_single_value_is_exact() {
        assert!(matches!(
            parse_expr("[21.0.5]"),
            VersionExprNode::Exact { .. }
        ));
    }

    // ---- parsing: config lines ----

    #[test]
    fn config_line_no_vendor() {
        let line = make_line("java = 21");
        assert_eq!(line.candidate, "java");
        assert!(line.vendor.is_none());
    }

    #[test]
    fn config_line_with_vendor() {
        let line = make_line("java = 21 tem");
        assert_eq!(line.candidate, "java");
        assert_eq!(line.vendor.as_deref(), Some("tem"));
    }

    #[test]
    fn config_line_range_expr() {
        let line = make_line("maven = [3.9,4)");
        assert_eq!(line.candidate, "maven");
        assert!(matches!(line.expr, VersionExprNode::Range { .. }));
    }

    #[test]
    fn config_line_whitespace_trimmed() {
        let line = make_line("  java = 21  ");
        assert_eq!(line.candidate, "java");
    }

    #[test]
    fn config_line_missing_equals_is_error() {
        assert!(ConfigLineParser::new("java 21", 1).parse_line().is_err());
    }

    // ---- parsing: document ----

    #[test]
    fn document_skips_blank_lines_and_comments() {
        let doc = parse_document("# comment\n\nmaven = [3.9.14]\njava 21\n");
        assert_eq!(doc.entries.len(), 2);
        assert_eq!(doc.entries[0].source, "maven = [3.9.14]");
        assert_eq!(doc.entries[1].source, "java 21");
    }

    #[test]
    fn document_line_numbers_are_correct() {
        let doc = parse_document("# comment\n\nmaven = [3.9.14]\njava 21\n");
        assert_eq!(doc.entries[0].line_number, 3);
        assert_eq!(doc.entries[1].line_number, 4);
    }

    // ---- parsing: sdk list (generic grid) ----

    #[test]
    fn generic_sdk_list_parses_status_flags() {
        let fixture = "================================================================================\nAvailable Demo Versions\n================================================================================\n > + 1.2.3               * 1.2.2             + 1.2.1                            \n\n================================================================================\n+ - local version\n* - installed\n> - currently in use\n================================================================================\n";
        let node = parse_sdk_list("demo", fixture);
        assert_eq!(node.rows.len(), 3);
        assert_eq!(node.rows[0].version, "1.2.3");
        assert_eq!(node.rows[0].status.as_deref(), Some("current local only"));
        assert_eq!(node.rows[1].status.as_deref(), Some("installed"));
        assert_eq!(node.rows[2].status.as_deref(), Some("local only"));
    }

    // ---- parsing: sdk list (java pipe table) ----

    #[test]
    fn java_sdk_list_parses_vendor_dist_and_in_use() {
        let fixture = "================================================================================\nAvailable Java Versions for Test Platform\n================================================================================\n Vendor        | Use | Version      | Dist    | Status     | Identifier\n--------------------------------------------------------------------------------\n GraalVM CE    | >>> | 25.0.1       | graalce | local only | 25.0.1-graalce\n               |     | 24.0.2       | graalce | installed  | 24.0.2-graalce\n";
        let node = parse_sdk_list("java", fixture);
        assert_eq!(node.rows.len(), 2);
        assert_eq!(node.rows[0].vendor_label.as_deref(), Some("GraalVM CE"));
        assert_eq!(node.rows[0].dist.as_deref(), Some("graalce"));
        assert!(node.rows[0].in_use);
        assert_eq!(
            node.rows[0].identifier.as_deref(),
            Some("25.0.1-graalce")
        );
        // Vendor carries over to continuation row
        assert_eq!(node.rows[1].vendor_label.as_deref(), Some("GraalVM CE"));
        assert_eq!(node.rows[1].dist.as_deref(), Some("graalce"));
        assert!(!node.rows[1].in_use);
        assert_eq!(
            node.rows[1].identifier.as_deref(),
            Some("24.0.2-graalce")
        );
    }

    // ---- version comparison: numerics ----

    #[test]
    fn numeric_comparison_is_not_lexicographic() {
        assert!(cmp("9", "10") < 0, "9 should be less than 10");
        assert!(cmp("3.9.9", "3.9.14") < 0, "3.9.9 should be less than 3.9.14");
    }

    // ---- version comparison: pre-release ordering ----

    #[test]
    fn prerelease_sorts_before_plain_release() {
        assert!(cmp("26.ea.35", "26") < 0);
        assert!(cmp("1.0.alpha", "1.0") < 0);
        assert!(cmp("1.0.snapshot", "1.0") < 0);
    }

    #[test]
    fn prerelease_order_within_qualifiers() {
        // alpha < beta < milestone < rc < ea < preview < snapshot < release
        assert!(cmp("1.0.alpha", "1.0.beta") < 0);
        assert!(cmp("1.0.beta", "1.0.milestone") < 0);
        assert!(cmp("1.0.milestone", "1.0.rc") < 0);
        assert!(cmp("1.0.rc", "1.0.ea") < 0);
        assert!(cmp("1.0.ea", "1.0.preview") < 0);
        assert!(cmp("1.0.preview", "1.0.snapshot") < 0);
    }

    #[test]
    fn prerelease_aliases_have_same_rank() {
        // a == alpha, b == beta, m == milestone, cr == rc
        // All four aliases sort with the same rank as their canonical form
        assert!(cmp("1.0.a", "1.0.beta") < 0);
        assert!(cmp("1.0.alpha", "1.0.beta") < 0);
        assert!(cmp("1.0.b", "1.0.milestone") < 0);
        assert!(cmp("1.0.beta", "1.0.milestone") < 0);
        assert!(cmp("1.0.m", "1.0.rc") < 0);
        assert!(cmp("1.0.milestone", "1.0.rc") < 0);
        assert!(cmp("1.0.cr", "1.0.ea") < 0);
        assert!(cmp("1.0.rc", "1.0.ea") < 0);
    }

    // ---- version comparison: release aliases ----

    #[test]
    fn release_alias_skipped_in_unit_comparison_but_ordered_by_string() {
        // Units are equal (Final/ga/release are skipped), but string tiebreaker
        // places the alias-suffixed form after the plain version.
        assert!(cmp("2.16.0.Final", "2.16.0") > 0);
        assert!(cmp("2.16.0.ga", "2.16.0") > 0);
        assert!(cmp("2.16.0.release", "2.16.0") > 0);
    }

    // ---- version comparison: variants ----

    #[test]
    fn variant_qualifiers_sort_after_plain_release() {
        assert!(cmp("21.0.10", "21.0.10.fx") < 0);
        assert!(cmp("21.0.10", "21.0.10.crac") < 0);
    }

    // ---- version comparison: post-release ----

    #[test]
    fn post_release_suffix_sorts_after_base() {
        assert!(cmp("25.0.2", "25.0.2.r25") < 0);
        assert!(cmp("5.0.0_1", "5.0.0_2") < 0);
        assert!(cmp("1.0.1-1", "1.0.1-2") < 0);
    }

    // ---- version comparison: numeric pre-release suffixes ----

    #[test]
    fn prerelease_with_numeric_suffix_ordered_numerically() {
        assert!(cmp("1.0.rc1", "1.0.rc2") < 0);
        assert!(cmp("26.ea.13", "26.ea.35") < 0);
        assert!(cmp("1.0.M1", "1.0.M2") < 0);
    }

    // ---- version comparison: exhaustion rules ----

    #[test]
    fn trailing_zero_is_greater_than_absent() {
        // 9.0 has no third component; 9.0.0 does — the extra zero is not elided
        assert!(cmp("9.0", "9.0.0") < 0);
    }

    #[test]
    fn plain_release_beats_prerelease_when_one_side_exhausts() {
        // 9.0 exhausts while 9.0.rc1 still has a pre-release token: 9.0 wins
        assert!(cmp("9.0", "9.0.rc1") > 0);
    }

    // ---- range membership ----

    #[test]
    fn stable_versions_match_plain_range() {
        assert!(expr_matches("[26,)", "26"));
        assert!(expr_matches("[26,)", "27"));
        assert!(expr_matches("[26,)", "27.0.1"));
    }

    #[test]
    fn prerelease_excluded_from_plain_range() {
        assert!(!expr_matches("[26,)", "26.ea.35"));
        assert!(!expr_matches("[26,)", "27.ea.14"));
    }

    #[test]
    fn prerelease_included_when_lower_bound_opts_in() {
        assert!(expr_matches("[26.ea,)", "26.ea.13"));
        assert!(expr_matches("[26.ea,)", "26.ea.35"));
        assert!(expr_matches("[26.ea,)", "27")); // stable always passes
    }

    #[test]
    fn prerelease_opt_in_is_base_specific() {
        // [26.ea,) opts in for 26.ea.* only, not for 27.ea.*
        assert!(!expr_matches("[26.ea,)", "27.ea.14"));
    }

    #[test]
    fn upper_bound_prerelease_opts_in_for_same_base() {
        assert!(expr_matches("[26.1,27.ea]", "27.ea"));
        assert!(!expr_matches("[26.1,27.ea]", "27.ea.14")); // exceeds upper bound
        assert!(!expr_matches("[26.1,27.ea]", "26.ea.35")); // different base (26 ≠ 27)
    }

    #[test]
    fn variant_qualifiers_included_in_containing_range() {
        assert!(expr_matches("[21.0.10,21.0.11)", "21.0.10"));
        assert!(expr_matches("[21.0.10,21.0.11)", "21.0.10.fx"));
        assert!(expr_matches("[21.0.10,21.0.11)", "21.0.10.crac"));
    }

    #[test]
    fn exact_expression_matches_release_aliases() {
        assert!(expr_matches("[2.16.0]", "2.16.0.Final"));
        assert!(expr_matches("[2.16.0]", "2.16.0"));
        assert!(expr_matches("[2.16.0]", "2.16.0.ga"));
    }

    #[test]
    fn exclusive_bounds_exclude_bound_values() {
        assert!(!expr_matches("[21,22)", "22")); // exclusive upper
        assert!(!expr_matches("(21,22]", "21")); // exclusive lower
        assert!(expr_matches("[21,22)", "21")); // inclusive lower
        assert!(expr_matches("(21,22]", "22")); // inclusive upper
    }

    // ---- vendor matching ----

    #[test]
    fn no_vendor_on_line_matches_any_dist() {
        let resolver = Resolver;
        let line = make_line("java = [21.0.2] ");
        let list = sdk_list("java", &["21.0.2-tem", "21.0.2-graalce"]);
        // Both rows match because no vendor is specified
        let count = list
            .rows
            .iter()
            .filter(|row| resolver.vendor_matches(&line, row))
            .count();
        assert_eq!(count, 2);
    }

    #[test]
    fn vendor_requires_exact_dist_match() {
        let resolver = Resolver;
        let line = make_line("java = [21] graalce");
        let list = sdk_list("java", &["21.0.2-tem", "21.0.2-graalce"]);
        let matched: Vec<_> = list
            .rows
            .iter()
            .filter(|row| resolver.vendor_matches(&line, row))
            .collect();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].dist.as_deref(), Some("graalce"));
    }

    #[test]
    fn vendor_match_is_case_sensitive() {
        let resolver = Resolver;
        let line = make_line("java = [21] GraalCE");
        let list = sdk_list("java", &["21.0.2-graalce"]);
        // "GraalCE" != "graalce"
        let any_match = list
            .rows
            .iter()
            .any(|row| resolver.vendor_matches(&line, row));
        assert!(!any_match);
    }

    // ---- resolution ----

    #[test]
    fn resolves_highest_matching_version() {
        let resolver = Resolver;
        let line = make_line("demo = [1,2)");
        let list = sdk_list("demo", &["1.0.0", "1.0.2", "1.0.1"]);
        let result = resolver.resolve_line(&line, &list).unwrap();
        assert_eq!(result.version, "1.0.2");
    }

    #[test]
    fn vendor_filter_selects_matching_dist() {
        let resolver = Resolver;
        let line = make_line("java = [21,22) tem");
        let list = sdk_list("java", &["21.0.2-graalce", "21.0.2-tem"]);
        let result = resolver.resolve_line(&line, &list).unwrap();
        assert_eq!(result.dist.as_deref(), Some("tem"));
    }

    #[test]
    fn no_matching_version_returns_error() {
        let resolver = Resolver;
        let line = make_line("demo = [2,3)");
        let list = sdk_list("demo", &["1.0.0"]);
        assert!(resolver.resolve_line(&line, &list).is_err());
    }

    #[test]
    fn empty_sdk_list_returns_error() {
        let resolver = Resolver;
        let line = make_line("demo = [1,2)");
        let list = sdk_list("demo", &[]);
        assert!(resolver.resolve_line(&line, &list).is_err());
    }

    #[test]
    fn identifier_used_as_resolution_target() {
        let resolver = Resolver;
        let line = make_line("java = [21.0.2] tem");
        let list = sdk_list("java", &["21.0.2-tem"]);
        let result = resolver.resolve_line(&line, &list).unwrap();
        // target should be the identifier "21.0.2-tem", not just the version "21.0.2"
        assert_eq!(result.target, "21.0.2-tem");
        assert_eq!(result.version, "21.0.2");
    }

    #[test]
    fn continues_after_malformed_document_entries() {
        let doc = parse_document("demo = [1.2.3]\njava 21\n");
        let resolver = Resolver;
        let list = sdk_list("demo", &["1.2.3"]);
        let mut successes = 0;
        let mut errors = 0;
        for entry in doc.entries {
            match ConfigLineParser::new(&entry.source, entry.line_number).parse_line() {
                Ok(config) if config.candidate == "demo" => {
                    match resolver.resolve_line(&config, &list) {
                        Ok(_) => successes += 1,
                        Err(_) => errors += 1,
                    }
                }
                Ok(_) => errors += 1, // java 21 has no sdk list available
                Err(_) => errors += 1,
            }
        }
        assert_eq!(successes, 1);
        assert_eq!(errors, 1);
    }

    // ---- parsing: sdk list (fixtures) ----

    fn load_fixture(candidate: &str) -> SdkListNode {
        let text = match candidate {
            "java"       => include_str!("../tests/fixtures/sdk_list/java.txt"),
            "gradle"     => include_str!("../tests/fixtures/sdk_list/gradle.txt"),
            "maven"      => include_str!("../tests/fixtures/sdk_list/maven.txt"),
            "kotlin"     => include_str!("../tests/fixtures/sdk_list/kotlin.txt"),
            "scala"      => include_str!("../tests/fixtures/sdk_list/scala.txt"),
            "groovy"     => include_str!("../tests/fixtures/sdk_list/groovy.txt"),
            "ant"        => include_str!("../tests/fixtures/sdk_list/ant.txt"),
            "springboot" => include_str!("../tests/fixtures/sdk_list/springboot.txt"),
            "micronaut"  => include_str!("../tests/fixtures/sdk_list/micronaut.txt"),
            "sbt"        => include_str!("../tests/fixtures/sdk_list/sbt.txt"),
            other        => panic!("no fixture for {other}"),
        };
        parse_sdk_list(candidate, text)
    }

    /// Reset all statuses in a parsed SdkListNode, then mark specific identifiers
    /// as installed. Used in resolver tests to simulate a clean installed state
    /// without depending on what was actually installed at fixture capture time.
    fn with_installed(mut sdk: SdkListNode, installed: &[&str], in_use: &str) -> SdkListNode {
        for row in &mut sdk.rows {
            row.status = None;
            row.in_use = false;
            let id = row.identifier.as_deref().unwrap_or(&row.version);
            if id == in_use {
                row.status = Some("current local only".to_string());
                row.in_use = true;
            } else if installed.contains(&id) {
                row.status = Some("local only".to_string());
            }
        }
        sdk
    }

    // Java pipe-table fixture tests

    #[test]
    fn java_fixture_parses_to_nonempty_list() {
        assert!(!load_fixture("java").rows.is_empty());
    }

    #[test]
    fn java_fixture_has_multiple_vendors() {
        let sdk = load_fixture("java");
        let vendor_count = sdk.rows.iter()
            .filter_map(|r| r.vendor_label.as_deref())
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert!(vendor_count >= 3, "expected at least 3 distinct vendors, got {vendor_count}");
    }

    #[test]
    fn java_fixture_vendor_carries_down_to_all_rows() {
        let sdk = load_fixture("java");
        for row in &sdk.rows {
            assert!(
                row.vendor_label.is_some(),
                "row {} {} has no vendor_label",
                row.version,
                row.dist.as_deref().unwrap_or("")
            );
        }
    }

    #[test]
    fn java_fixture_identifiers_have_dist_suffix() {
        let sdk = load_fixture("java");
        for row in &sdk.rows {
            if let Some(id) = &row.identifier {
                assert!(
                    id.contains('-'),
                    "identifier {id:?} has no dist suffix"
                );
            }
        }
    }

    #[test]
    fn java_fixture_dist_matches_identifier_suffix() {
        let sdk = load_fixture("java");
        for row in &sdk.rows {
            if let (Some(id), Some(dist)) = (&row.identifier, &row.dist) {
                assert!(
                    id.ends_with(&format!("-{dist}")),
                    "identifier {id:?} does not end with -{dist}"
                );
            }
        }
    }

    #[test]
    fn java_fixture_all_versions_parse() {
        let sdk = load_fixture("java");
        for row in &sdk.rows {
            VersionParser::new(&row.version)
                .parse_version()
                .unwrap_or_else(|e| panic!("version {:?} failed to parse: {e}", row.version));
        }
    }

    #[test]
    fn java_fixture_installed_versions_detected() {
        let sdk = load_fixture("java");
        let installed: Vec<_> = sdk.rows.iter()
            .filter(|r| r.status.is_some())
            .collect();
        assert!(
            !installed.is_empty(),
            "expected at least one installed row in java fixture"
        );
    }

    #[test]
    fn java_fixture_in_use_row_is_graalce_25() {
        let sdk = load_fixture("java");
        let in_use: Vec<_> = sdk.rows.iter().filter(|r| r.in_use).collect();
        assert_eq!(in_use.len(), 1, "expected exactly one in-use row");
        assert_eq!(in_use[0].version, "25.0.2");
        assert_eq!(in_use[0].dist.as_deref(), Some("graalce"));
    }

    // Grid-format fixture tests

    #[test]
    fn gradle_fixture_parses_to_nonempty_list() {
        assert!(!load_fixture("gradle").rows.is_empty());
    }

    #[test]
    fn gradle_fixture_rows_have_no_dist() {
        let sdk = load_fixture("gradle");
        for row in &sdk.rows {
            assert!(row.dist.is_none(), "gradle row {} unexpectedly has dist", row.version);
        }
    }

    #[test]
    fn gradle_fixture_all_versions_parse() {
        let sdk = load_fixture("gradle");
        for row in &sdk.rows {
            VersionParser::new(&row.version)
                .parse_version()
                .unwrap_or_else(|e| panic!("gradle version {:?} failed to parse: {e}", row.version));
        }
    }

    #[test]
    fn gradle_fixture_installed_versions_detected() {
        let sdk = load_fixture("gradle");
        // 9.4.1 was current+installed and 8.14.4 was installed at capture time
        let installed: Vec<_> = sdk.rows.iter()
            .filter(|r| r.status.is_some())
            .collect();
        assert!(
            installed.len() >= 2,
            "expected at least 2 installed gradle versions, got {}",
            installed.len()
        );
    }

    #[test]
    fn maven_fixture_parses_to_nonempty_list() {
        assert!(!load_fixture("maven").rows.is_empty());
    }

    #[test]
    fn maven_fixture_all_versions_parse() {
        let sdk = load_fixture("maven");
        for row in &sdk.rows {
            VersionParser::new(&row.version)
                .parse_version()
                .unwrap_or_else(|e| panic!("maven version {:?} failed to parse: {e}", row.version));
        }
    }

    #[test]
    fn kotlin_fixture_parses_to_nonempty_list() {
        assert!(!load_fixture("kotlin").rows.is_empty());
    }

    #[test]
    fn kotlin_fixture_all_versions_parse() {
        let sdk = load_fixture("kotlin");
        for row in &sdk.rows {
            VersionParser::new(&row.version)
                .parse_version()
                .unwrap_or_else(|e| panic!("kotlin version {:?} failed to parse: {e}", row.version));
        }
    }

    #[test]
    fn scala_fixture_parses_to_nonempty_list() {
        assert!(!load_fixture("scala").rows.is_empty());
    }

    #[test]
    fn scala_fixture_all_versions_parse() {
        let sdk = load_fixture("scala");
        for row in &sdk.rows {
            VersionParser::new(&row.version)
                .parse_version()
                .unwrap_or_else(|e| panic!("scala version {:?} failed to parse: {e}", row.version));
        }
    }

    // Cross-cutting fixture tests

    #[test]
    fn all_fixture_candidates_parse_to_nonempty_lists() {
        let candidates = [
            "java", "gradle", "maven", "kotlin", "scala",
            "groovy", "ant", "springboot", "micronaut", "sbt",
        ];
        for candidate in candidates {
            let sdk = load_fixture(candidate);
            assert!(
                !sdk.rows.is_empty(),
                "fixture for {candidate} parsed to empty list"
            );
        }
    }

    #[test]
    fn all_grid_fixture_versions_are_parseable() {
        let candidates = [
            "gradle", "maven", "kotlin", "scala",
            "groovy", "ant", "springboot", "micronaut", "sbt",
        ];
        for candidate in candidates {
            let sdk = load_fixture(candidate);
            for row in &sdk.rows {
                VersionParser::new(&row.version)
                    .parse_version()
                    .unwrap_or_else(|e| panic!(
                        "{candidate} version {:?} failed to parse: {e}", row.version
                    ));
            }
        }
    }

    // Resolver integration tests against fixtures

    #[test]
    fn resolve_gradle_range_against_fixture() {
        // 8.14.4 was installed at capture time; range [8,9) should select it
        let sdk = load_fixture("gradle");
        let line = make_line("gradle = [8,9)");
        let resolved = Resolver.resolve_line(&line, &sdk).unwrap();
        assert!(
            resolved.target.starts_with("8."),
            "expected an 8.x version, got {:?}",
            resolved.target
        );
    }

    #[test]
    fn resolve_maven_range_against_fixture() {
        // 3.9.14 was installed at capture time; range [3.9,4) should select it
        let sdk = load_fixture("maven");
        let line = make_line("maven = [3.9,4)");
        let resolved = Resolver.resolve_line(&line, &sdk).unwrap();
        assert!(
            resolved.target.starts_with("3.9"),
            "expected a 3.9.x version, got {:?}",
            resolved.target
        );
    }

    #[test]
    fn resolve_java_exact_version_against_fixture() {
        // 21.0.10-tem was installed at capture time
        let sdk = load_fixture("java");
        let line = make_line("java = [21.0.10] tem");
        let resolved = Resolver.resolve_line(&line, &sdk).unwrap();
        assert_eq!(resolved.target, "21.0.10-tem");
    }

    #[test]
    fn resolve_against_fixture_using_with_installed() {
        // with_installed() allows resolver tests that don't depend on what was installed at
        // fixture capture time — useful for testing against arbitrary versions in the list
        let sdk = with_installed(load_fixture("gradle"), &["8.7", "9.4.1"], "9.4.1");
        let line = make_line("gradle = [8,9)");
        let resolved = Resolver.resolve_line(&line, &sdk).unwrap();
        assert_eq!(resolved.target, "8.7");
    }

    // ---- suggest install ----

    #[test]
    fn find_best_uninstalled_returns_best_matching_gradle() {
        // gradle fixture: 9.4.1 (current) and 8.14.4 are installed; other 8.x are not
        let sdk = load_fixture("gradle");
        let line = make_line("gradle = [8,9)");
        let row = Resolver.find_best_uninstalled(&line, &sdk).unwrap().unwrap();
        assert!(row.version.starts_with("8."), "expected 8.x, got {}", row.version);
        assert_ne!(row.version, "8.14.4"); // that one is installed
    }

    #[test]
    fn find_best_uninstalled_returns_none_when_no_version_matches() {
        let sdk = load_fixture("gradle");
        let line = make_line("gradle = 0.0.0-nonexistent");
        assert!(Resolver.find_best_uninstalled(&line, &sdk).unwrap().is_none());
    }

    #[test]
    fn find_best_uninstalled_skips_installed_rows() {
        let mut sdk = load_fixture("gradle");
        for row in &mut sdk.rows {
            row.status = Some("local only".to_string());
        }
        let line = make_line("gradle = [8,9)");
        assert!(Resolver.find_best_uninstalled(&line, &sdk).unwrap().is_none());
    }

    #[test]
    fn find_best_uninstalled_maven_range() {
        // maven fixture: 3.9.14 installed (current); 3.9.x others are uninstalled
        let sdk = load_fixture("maven");
        let line = make_line("maven = [3.9,4)");
        let row = Resolver.find_best_uninstalled(&line, &sdk).unwrap().unwrap();
        assert!(row.version.starts_with("3.9."), "expected 3.9.x, got {}", row.version);
        assert_ne!(row.version, "3.9.14"); // that one is installed
    }

    // ---- project discovery ----

    #[test]
    fn finds_sdkvers_file_in_ancestor_directory() {
        let temp_root = env::temp_dir().join(format!("sdkvers-test-{}", std::process::id()));
        let nested = temp_root.join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        let file_path = temp_root.join(".sdkvers");
        fs::write(&file_path, "java = 21\n").unwrap();
        let result = find_sdkvers_path(nested.to_string_lossy().as_ref());
        // Canonicalize while files still exist, then clean up.
        let found = result.map(|p| {
            fs::canonicalize(&p).unwrap_or_else(|_| PathBuf::from(&p))
        });
        let expected = fs::canonicalize(&file_path).unwrap_or(file_path);
        let _ = fs::remove_dir_all(&temp_root);
        assert_eq!(found.unwrap(), expected);
    }

    #[test]
    fn returns_error_when_no_sdkvers_file_exists() {
        let temp_root = env::temp_dir().join(format!("sdkvers-test-nofile-{}", std::process::id()));
        let nested = temp_root.join("a");
        fs::create_dir_all(&nested).unwrap();
        let result = find_sdkvers_path(nested.to_string_lossy().as_ref());
        let _ = fs::remove_dir_all(&temp_root);
        assert!(result.is_err());
    }

    #[test]
    fn ignores_sdkvers_directory_in_ancestor() {
        let temp_root = env::temp_dir().join(format!("sdkvers-test-dir-{}", std::process::id()));
        let nested = temp_root.join("a");
        fs::create_dir_all(&nested).unwrap();
        // Create a .sdkvers directory (not a file) in the ancestor — should be ignored.
        fs::create_dir_all(temp_root.join(".sdkvers")).unwrap();
        let result = find_sdkvers_path(nested.to_string_lossy().as_ref());
        let _ = fs::remove_dir_all(&temp_root);
        assert!(result.is_err());
    }
}
