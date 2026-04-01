use std::path::PathBuf;

use types::{ArchiveFormat, Candidate, Identifier, Platform};

use crate::{Error, client};

/// A downloaded archive held as a temporary file pending verification and extraction.
pub struct DownloadedArchive {
    pub path: PathBuf,
    pub format: ArchiveFormat,
    pub checksum_sha256: Option<String>,
    pub checksum_md5: Option<String>,
}

/// Download the archive for a specific candidate version to a temporary file.
/// Hits GET /broker/download/{candidate}/{identifier}/{platform}.
/// Checksums are read from response headers and returned with the archive.
pub fn download_archive(
    candidate: &Candidate,
    identifier: &Identifier,
    platform: &Platform,
) -> Result<DownloadedArchive, Error> {
    let path = format!(
        "/broker/download/{}/{}/{}",
        candidate.as_str(),
        identifier.as_str(),
        platform.as_api_str()
    );

    let dest = temp_path(candidate, identifier);
    let headers = client::download(&path, &dest)?;

    let format = detect_format(&dest);

    Ok(DownloadedArchive {
        path: dest,
        format,
        checksum_sha256: headers.sha256,
        checksum_md5: headers.md5,
    })
}

/// Detect archive format from file extension or content.
fn detect_format(path: &std::path::Path) -> ArchiveFormat {
    let name = path.to_string_lossy();
    if name.ends_with(".zip") {
        ArchiveFormat::Zip
    } else {
        // Default to TarGz; Content-Type detection can be added later.
        ArchiveFormat::TarGz
    }
}

fn temp_path(candidate: &Candidate, identifier: &Identifier) -> PathBuf {
    std::env::temp_dir().join(format!("sdkvers-{}-{}", candidate.as_str(), identifier.as_str()))
}
