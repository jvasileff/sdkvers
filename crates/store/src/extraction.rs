use std::path::{Path, PathBuf};

use types::{Candidate, HookFingerprint, Identifier};

use crate::{Error, candidates_dir};

/// Return the filesystem path for an installed version.
pub fn version_path(candidate: &Candidate, identifier: &Identifier) -> Result<PathBuf, Error> {
    Ok(candidates_dir()?.join(candidate.as_str()).join(identifier.as_str()))
}

/// Verify the checksum of a downloaded archive.
/// Prefers SHA-256; falls back to MD5. Logs a warning if neither is available.
pub fn verify_checksum(
    path: &Path,
    sha256: Option<&str>,
    md5: Option<&str>,
) -> Result<(), Error> {
    if let Some(expected) = sha256 {
        let actual = sha256_hex(path)?;
        if actual != expected {
            return Err(Error::ChecksumMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
        return Ok(());
    }
    if let Some(expected) = md5 {
        let actual = md5_hex(path)?;
        if actual != expected {
            return Err(Error::ChecksumMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
        return Ok(());
    }
    // No checksums provided — proceed without verification.
    eprintln!("sdkalt: warning: no checksum available for download, skipping verification");
    Ok(())
}

/// Extract a downloaded archive to $SDKMAN_DIR/candidates/{candidate}/{identifier}/.
/// Extraction behaviour is determined by the hook fingerprint.
/// Deletes the archive file on both success and failure.
pub fn extract(
    candidate: &Candidate,
    identifier: &Identifier,
    archive: &Path,
    hook: &HookFingerprint,
) -> Result<(), Error> {
    let dest = version_path(candidate, identifier)?;
    std::fs::create_dir_all(&dest)?;

    let result = do_extract(candidate, identifier, archive, hook, &dest);

    // Always remove the archive.
    let _ = std::fs::remove_file(archive);

    if result.is_err() {
        // Clean up partial extraction.
        let _ = std::fs::remove_dir_all(&dest);
    }

    result
}

fn do_extract(
    candidate: &Candidate,
    identifier: &Identifier,
    archive: &Path,
    hook: &HookFingerprint,
    dest: &Path,
) -> Result<(), Error> {
    match hook {
        HookFingerprint::Unknown { hash } => {
            return Err(Error::UnknownHookFingerprint {
                candidate: candidate.to_string(),
                identifier: identifier.to_string(),
                hash: hash.clone(),
            });
        }

        HookFingerprint::DefaultZip => {
            extract_zip_strip_leading(archive, dest, candidate, identifier)?;
        }

        HookFingerprint::DefaultTarball | HookFingerprint::LinuxJavaTarball => {
            extract_targz_strip_leading(archive, dest, candidate, identifier)?;
        }

        HookFingerprint::OsxJavaTarball => {
            extract_targz_contents_home(archive, dest, candidate, identifier)?;
        }

        HookFingerprint::UnixJmcTarballFolder { executable_binary } => {
            extract_targz_strip_leading(archive, dest, candidate, identifier)?;
            create_jmc_symlink(dest, executable_binary, candidate, identifier)?;
        }

        HookFingerprint::UnixJmcTarballFlat { executable_binary } => {
            extract_targz_flat(archive, dest)?;
            create_jmc_symlink(dest, executable_binary, candidate, identifier)?;
        }
    }

    // Sanity check: destination should not be empty.
    if std::fs::read_dir(dest)?.next().is_none() {
        return Err(Error::UnexpectedLayout {
            candidate: candidate.to_string(),
            identifier: identifier.to_string(),
            detail: "extraction produced an empty directory".to_string(),
        });
    }

    Ok(())
}

/// Extract a zip archive, stripping a single leading directory if present.
fn extract_zip_strip_leading(
    archive: &Path,
    dest: &Path,
    candidate: &Candidate,
    identifier: &Identifier,
) -> Result<(), Error> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| Error::UnexpectedLayout {
        candidate: candidate.to_string(),
        identifier: identifier.to_string(),
        detail: format!("zip open error: {e}"),
    })?;

    let strip = leading_dir_zip(&mut zip);

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Error::UnexpectedLayout {
            candidate: candidate.to_string(),
            identifier: identifier.to_string(),
            detail: format!("zip entry error: {e}"),
        })?;

        let entry_path = match entry.enclosed_name() {
            Some(p) => p.to_owned(),
            None => continue,
        };

        let relative = match strip {
            Some(ref prefix) => match entry_path.strip_prefix(prefix) {
                Ok(r) => r.to_owned(),
                Err(_) => continue,
            },
            None => entry_path,
        };

        if relative.as_os_str().is_empty() {
            continue;
        }

        let out = dest.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut outfile)?;
            set_permissions(&out, entry.unix_mode())?;
        }
    }
    Ok(())
}

/// Extract a tar.gz archive, stripping a single leading directory if present.
fn extract_targz_strip_leading(
    archive: &Path,
    dest: &Path,
    candidate: &Candidate,
    identifier: &Identifier,
) -> Result<(), Error> {
    let file = std::fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);

    // Collect entries to detect leading directory.
    let entries: Vec<PathBuf> = tar
        .entries()?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.path().ok().map(|p| p.to_path_buf()))
        .collect();

    let strip = leading_dir_tar(&entries);

    // Re-open archive for actual extraction.
    let file = std::fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);

    for entry in tar.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.to_owned();
        let relative = match &strip {
            Some(prefix) => match entry_path.strip_prefix(prefix) {
                Ok(r) => r.to_owned(),
                Err(_) => continue,
            },
            None => entry_path.to_path_buf(),
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&relative);
        entry.set_preserve_permissions(true);
        entry.unpack(&out).map_err(|e| Error::UnexpectedLayout {
            candidate: candidate.to_string(),
            identifier: identifier.to_string(),
            detail: format!("tar unpack error: {e}"),
        })?;
    }
    Ok(())
}

/// Extract a tar.gz archive using Contents/Home/ as the extraction root (macOS Java).
fn extract_targz_contents_home(
    archive: &Path,
    dest: &Path,
    candidate: &Candidate,
    identifier: &Identifier,
) -> Result<(), Error> {
    let file = std::fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);

    let mut found_home = false;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.to_owned();

        // Find the Contents/Home/ segment and use Home/ as root.
        let home_root = find_contents_home(&entry_path);
        if let Some(relative) = home_root {
            found_home = true;
            if relative.as_os_str().is_empty() {
                continue;
            }
            let out = dest.join(relative);
            entry.set_preserve_permissions(true);
            entry.unpack(&out).map_err(|e| Error::UnexpectedLayout {
                candidate: candidate.to_string(),
                identifier: identifier.to_string(),
                detail: format!("tar unpack error: {e}"),
            })?;
        }
    }

    if !found_home {
        return Err(Error::UnexpectedLayout {
            candidate: candidate.to_string(),
            identifier: identifier.to_string(),
            detail: "Contents/Home/ not found in archive".to_string(),
        });
    }
    Ok(())
}

/// Extract a flat tar.gz archive (no leading directory) directly to dest.
fn extract_targz_flat(archive: &Path, dest: &Path) -> Result<(), Error> {
    let file = std::fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    tar.set_preserve_permissions(true);
    tar.unpack(dest)?;
    Ok(())
}

/// Create the bin/jmc symlink required for JMC installations.
fn create_jmc_symlink(
    dest: &Path,
    executable_binary: &str,
    _candidate: &Candidate,
    _identifier: &Identifier,
) -> Result<(), Error> {
    let bin_dir = dest.join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    let link = bin_dir.join("jmc");
    let target = format!("../{executable_binary}");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link)?;

    #[cfg(not(unix))]
    return Err(Error::UnexpectedLayout {
        candidate: _candidate.to_string(),
        identifier: _identifier.to_string(),
        detail: "JMC symlink creation not supported on this platform".to_string(),
    });

    Ok(())
}

/// Remove an installed version directory.
pub fn remove(candidate: &Candidate, identifier: &Identifier) -> Result<(), Error> {
    let path = version_path(candidate, identifier)?;
    if !path.exists() {
        return Err(Error::VersionNotInstalled {
            candidate: candidate.to_string(),
            identifier: identifier.to_string(),
        });
    }
    std::fs::remove_dir_all(&path)?;
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Detect a single common leading directory in a zip archive, if present.
fn leading_dir_zip(zip: &mut zip::ZipArchive<std::fs::File>) -> Option<PathBuf> {
    let mut prefix: Option<PathBuf> = None;
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index_raw(i) {
            if let Some(p) = entry.enclosed_name() {
                let first = p.components().next()?.as_os_str().to_owned();
                match &prefix {
                    None => prefix = Some(PathBuf::from(&first)),
                    Some(existing) if existing.as_os_str() != first => return None,
                    _ => {}
                }
            }
        }
    }
    prefix
}

/// Detect a single common leading directory in a list of tar entry paths.
fn leading_dir_tar(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut prefix: Option<&std::ffi::OsStr> = None;
    for path in paths {
        let first = path.components().next()?.as_os_str();
        match prefix {
            None => prefix = Some(first),
            Some(p) if p != first => return None,
            _ => {}
        }
    }
    prefix.map(PathBuf::from)
}

/// Given a tar entry path, return the portion after Contents/Home/ if present.
fn find_contents_home(path: &Path) -> Option<PathBuf> {
    let components: Vec<_> = path.components().collect();
    for i in 0..components.len().saturating_sub(1) {
        if components[i].as_os_str() == "Contents"
            && components[i + 1].as_os_str() == "Home"
        {
            let rest: PathBuf = components[i + 2..].iter().collect();
            return Some(rest);
        }
    }
    None
}

/// Compute the SHA-256 hex digest of a file.
fn sha256_hex(path: &Path) -> Result<String, Error> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute the MD5 hex digest of a file.
fn md5_hex(path: &Path) -> Result<String, Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(format!("{:x}", md5::compute(&buf)))
}

#[cfg(unix)]
fn set_permissions(path: &Path, mode: Option<u32>) -> Result<(), Error> {
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_permissions(_path: &Path, _mode: Option<u32>) -> Result<(), Error> {
    Ok(())
}
