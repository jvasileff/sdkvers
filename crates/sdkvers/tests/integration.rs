use sdkvers::*;
use std::path::PathBuf;
use types::Platform;

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
#[ignore = "requires network access; run to regenerate fixtures"]
fn capture_sdk_list_fixtures() {
    let candidates = [
        "java", "gradle", "maven", "kotlin", "scala",
        "groovy", "ant", "springboot", "micronaut", "sbt",
    ];
    let platform = Platform::current().unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sdk_list");
    std::fs::create_dir_all(&dir).unwrap();
    for candidate_name in candidates {
        let candidate = Candidate::new(candidate_name);
        match broker::list_versions_raw(&candidate, &platform) {
            Ok(text) => {
                std::fs::write(dir.join(format!("{candidate_name}.txt")), &text).unwrap();
                println!("captured {candidate_name}");
            }
            Err(e) => eprintln!("skipping {candidate_name}: {e}"),
        }
    }
}

#[test]
#[ignore = "requires network access to SDKMAN API"]
fn fetches_live_java_versions() {
    let platform = Platform::current().unwrap();
    let candidate = Candidate::new("java");
    let sdk = broker::list_versions(&candidate, &platform).unwrap();
    assert!(!sdk.rows.is_empty(), "expected at least one row in live sdk list");
}
