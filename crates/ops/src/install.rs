use types::{Candidate, Identifier, Platform, Resolver, VersionExprNode};

use crate::Error;

/// Install a candidate version matching the given expression and optional vendor filter.
pub fn install(
    candidate: &Candidate,
    expr: &VersionExprNode,
    vendor_filter: Option<&str>,
) -> Result<Identifier, Error> {
    let platform = Platform::current()?;
    let sdk = broker::list_versions(candidate, &platform)?;
    let resolver = Resolver;

    let mut best: Option<&types::SdkListRow> = None;
    for row in &sdk.rows {
        if let Some(vf) = vendor_filter {
            if row.dist.as_deref() != Some(vf) {
                continue;
            }
        }
        if !resolver.version_expr_matches(expr, &row.version).unwrap_or(false) {
            continue;
        }
        if let Some(current) = best {
            if resolver.compare_versions(&row.version, &current.version).unwrap_or(0) > 0 {
                best = Some(row);
            }
        } else {
            best = Some(row);
        }
    }

    let row = best.ok_or_else(|| Error::NoMatch {
        candidate: candidate.to_string(),
        expr: expr.source().to_string(),
    })?;

    let identifier = Identifier::new(
        row.identifier.clone().unwrap_or_else(|| row.version.clone())
    );

    if store::version_path(candidate, &identifier).map(|p| p.exists()).unwrap_or(false) {
        return Err(Error::AlreadyInstalled {
            candidate: candidate.to_string(),
            identifier: identifier.to_string(),
        });
    }

    let hook = broker::fetch_hook(candidate, &identifier, &platform)?;
    let dl = broker::download_archive(candidate, &identifier, &platform)?;
    store::verify_checksum(&dl.path, dl.checksum_sha256.as_deref(), dl.checksum_md5.as_deref())?;
    store::extract(candidate, &identifier, &dl.path, &hook.fingerprint)?;

    Ok(identifier)
}
