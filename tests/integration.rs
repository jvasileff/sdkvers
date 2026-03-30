use sdkvers::*;

#[test]
#[ignore = "requires SDKMAN installation"]
fn loads_local_java_versions() {
    let sdk = load_local_sdk_list("java").unwrap();
    assert!(
        !sdk.rows.is_empty(),
        "expected at least one locally installed Java version"
    );
}

#[test]
#[ignore = "requires SDKMAN installation"]
fn resolves_installed_java_version() {
    let sdk = load_local_sdk_list("java").unwrap();
    let first = sdk.rows.first().expect("expected at least one Java version");
    let config_str = if let Some(dist) = &first.dist {
        format!("java = [{}] {}", first.version, dist)
    } else {
        format!("java = [{}]", first.version)
    };
    let config = ConfigLineParser::new(&config_str, 1).parse_line().unwrap();
    let r = Resolver;
    let resolved = r.resolve_line(&config, &sdk).unwrap();
    let expected_target = first
        .identifier
        .clone()
        .unwrap_or_else(|| first.version.clone());
    assert_eq!(resolved.target, expected_target);
}

#[test]
#[ignore = "requires SDKMAN installation and network"]
fn runs_live_sdk_list() {
    let text = run_sdk_list("java").unwrap();
    assert!(
        text.contains("Available Java Versions"),
        "unexpected sdk list output"
    );
    let sdk = parse_sdk_list("java", &text);
    assert!(!sdk.rows.is_empty(), "expected at least one row in live sdk list");
}
