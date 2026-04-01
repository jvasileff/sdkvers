use sdkvers::{
    ConfigLineParser, Resolver, VersionParser, bootstrap_sdkvers_content, dump_config_line,
    dump_document, dump_sdk_list, dump_version, dump_version_expr, find_sdkvers_path,
    load_local_sdk_list, parse_document, parse_sdk_list, read_utf8_file,
    resolve_document_with_details, run_sdk_list, self_test, suggest_install,
};
use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

// ---------------------------------------------------------------------------
// Shell-function output protocol
//
// The `sdkvers` subcommand writes two Quoted-Printable encoded sections
// separated by a per-invocation random UUID.  The shell function pipes this
// output through `sdkvers-resolve extract <eval|stdout>` to route and
// decode each section.  Stderr is written directly by the binary and is never
// part of the protocol.
//
// Format:
//   {uuid}\n{qp(eval)}\n{uuid}\n{qp(stdout)}\n{uuid}
//
// The separator between sections is the byte sequence \n{uuid}\n.
// The closing \n{uuid} (no trailing newline) ensures the output never ends
// with a newline, preventing shell $() from stripping content.
// The header {uuid}\n lets `extract` learn the separator without any
// out-of-band knowledge.
//
// QP encoding (RFC 2045, LF line endings) ensures sections survive shell
// variable capture: NUL bytes become =00, other non-printable bytes become
// =XX, and soft line breaks keep lines ≤76 chars for readability.
// The UUID uses only hex digits and dashes, which QP never encodes, so it
// cannot appear verbatim inside encoded content.
//
// The shell function only processes the protocol when the binary exits 0.
// On non-zero exit the binary has already written its error(s) to stderr.
// ---------------------------------------------------------------------------

fn generate_fn_uuid() -> String {
    use std::io::Read;
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .expect("failed to read /dev/urandom");
    // Set UUID v4 version and variant bits
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        u16::from_be_bytes([b[4], b[5]]),
        u16::from_be_bytes([b[6], b[7]]),
        u16::from_be_bytes([b[8], b[9]]),
        u64::from_be_bytes([0, 0, b[10], b[11], b[12], b[13], b[14], b[15]]),
    )
}

struct FnOutput {
    eval: String,
    stdout: String,
}

impl FnOutput {
    fn new() -> Self {
        FnOutput {
            eval: String::new(),
            stdout: String::new(),
        }
    }

    fn write(&self, uuid: &str) {
        let mut out = std::io::stdout();
        let _ = write!(out, "{uuid}\n");
        let _ = out.write_all(qp_encode(self.eval.as_bytes()).as_bytes());
        let _ = write!(out, "\n{uuid}\n");
        let _ = out.write_all(qp_encode(self.stdout.as_bytes()).as_bytes());
        // Closing delimiter: \n{uuid} with no trailing newline.  This ensures
        // the output always ends with a non-newline byte so that shell $()
        // command substitution cannot strip meaningful trailing content.
        let _ = write!(out, "\n{uuid}");
    }
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

fn print_top_help_to(w: &mut dyn Write) {
    let _ = writeln!(w, "usage: sdkvers-resolve <command> [args...]");
    let _ = writeln!(w);
    let _ = writeln!(w, "commands:");
    let _ = writeln!(w, "  sdkvers [args...]    backend for the sdkvers() shell function");
    let _ = writeln!(w, "  --version            print version");
    let _ = writeln!(w, "  help                 print this help");
}

fn print_debug_help_to(w: &mut dyn Write) {
    let _ = writeln!(w, "usage: sdkvers-resolve debug <subcommand> [args...]");
    let _ = writeln!(w);
    let _ = writeln!(w, "resolve subcommands:");
    let _ = writeln!(w, "  resolve-project [dir]               resolve .sdkvers from dir (default: .)");
    let _ = writeln!(w, "  resolve-file <path>...              resolve a specific .sdkvers file");
    let _ = writeln!(w, "  resolve-line <line>...              resolve one config line against local SDKs");
    let _ = writeln!(w);
    let _ = writeln!(w, "parse subcommands (for inspection and testing):");
    let _ = writeln!(w, "  parse-version <version>...          parse and dump a version string");
    let _ = writeln!(w, "  parse-expr <expr>...                parse and dump a version expression");
    let _ = writeln!(w, "  parse-line <line>...                parse and dump a .sdkvers config line");
    let _ = writeln!(w, "  parse-file <path>...                parse and dump a .sdkvers file");
    let _ = writeln!(w, "  parse-sdkfile <candidate> <path>... parse sdk list output from a file");
    let _ = writeln!(w, "  parse-sdklist <candidate>...        run sdk list and parse its output");
    let _ = writeln!(w);
    let _ = writeln!(w, "other subcommands:");
    let _ = writeln!(w, "  self-test                           run the built-in test suite");
    let _ = writeln!(w, "  help                                print this help");
}

fn fn_help_text() -> &'static str {
    concat!(
        "sdkvers \u{2014} activate SDKMAN SDK versions for the current shell\n",
        "\n",
        "Run in a project directory to activate the versions in .sdkvers.\n",
        "\n",
        "  bootstrap [--directory <dir>]    create .sdkvers from active SDKMAN versions\n",
        "  selfupdate [check|force]         update sdkvers to the latest release\n",
        "  help                             show this message\n",
    )
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("error: {message}");
            }
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return Err("missing command; run 'sdkvers-resolve help' for usage".to_string());
    }

    match args[0].as_str() {
        "--version" => {
            println!("sdkvers-resolve {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "help" => {
            print_top_help_to(&mut std::io::stdout());
            Ok(())
        }
        "sdkvers" => run_fn(&args[1..]),
        "debug" => run_debug(&args[1..]),
        "internal" => run_internal(&args[1..]),
        other => Err(format!("unknown command: {other}")),
    }
}

fn run_debug(args: &[String]) -> Result<(), String> {
    let subcommand = match args.first().map(String::as_str) {
        None => {
            return Err(
                "missing subcommand; run 'sdkvers-resolve debug help' for usage".to_string(),
            );
        }
        Some(s) => s,
    };

    match subcommand {
        "help" => {
            print_debug_help_to(&mut std::io::stdout());
            Ok(())
        }
        "self-test" => {
            self_test(|name| eprintln!("  {name}")).map_err(|e| e.0)?;
            eprintln!("self-tests passed");
            Ok(())
        }
        "parse-version" => {
            for (i, value) in args[1..].iter().enumerate() {
                if i > 0 { println!(); }
                let parsed = VersionParser::new(value).parse_version().map_err(|e| e.0)?;
                print!("{}", dump_version(&parsed));
            }
            Ok(())
        }
        "parse-expr" => {
            for (i, value) in args[1..].iter().enumerate() {
                if i > 0 { println!(); }
                let parsed = VersionParser::new(value)
                    .parse_version_expr()
                    .map_err(|e| e.0)?;
                print!("{}", dump_version_expr(&parsed));
            }
            Ok(())
        }
        "parse-line" => {
            for (i, value) in args[1..].iter().enumerate() {
                if i > 0 { println!(); }
                let parsed = ConfigLineParser::new(value, 1).parse_line().map_err(|e| e.0)?;
                print!("{}", dump_config_line(&parsed));
            }
            Ok(())
        }
        "parse-file" => {
            for (i, path) in args[1..].iter().enumerate() {
                if i > 0 { println!(); }
                let document = parse_document(&read_utf8_file(path).map_err(|e| e.0)?);
                print!("{}", dump_document(&document));
            }
            Ok(())
        }
        "parse-sdkfile" => {
            let inputs = &args[1..];
            if inputs.len() < 2 || inputs.len() % 2 != 0 {
                return Err(
                    "usage: sdkvers-resolve debug parse-sdkfile <candidate> <path> [<candidate> <path> ...]"
                        .to_string(),
                );
            }
            let mut idx = 0;
            let mut first = true;
            while idx < inputs.len() {
                if !first { println!(); }
                first = false;
                let candidate = &inputs[idx];
                let path = &inputs[idx + 1];
                let node = parse_sdk_list(candidate, &read_utf8_file(path).map_err(|e| e.0)?);
                print!("{}", dump_sdk_list(&node));
                idx += 2;
            }
            Ok(())
        }
        "parse-sdklist" => {
            for (i, candidate) in args[1..].iter().enumerate() {
                if i > 0 { println!(); }
                let text = run_sdk_list(candidate).map_err(|e| e.0)?;
                let node = parse_sdk_list(candidate, &text);
                print!("{}", dump_sdk_list(&node));
            }
            Ok(())
        }
        "resolve-line" => {
            let resolver = Resolver;
            for (i, line_text) in args[1..].iter().enumerate() {
                if i > 0 { println!(); }
                let line = ConfigLineParser::new(line_text, 1).parse_line().map_err(|e| e.0)?;
                let sdk = load_local_sdk_list(&line.candidate).map_err(|e| e.0)?;
                match resolver.resolve_line(&line, &sdk) {
                    Ok(row) => println!("sdk use {} {}", row.candidate, row.target),
                    Err(e) => {
                        if let Some(hint) = suggest_install(&line) {
                            eprintln!("hint: {hint}");
                        }
                        return Err(e.0);
                    }
                }
            }
            Ok(())
        }
        "resolve-file" => {
            for path in &args[1..] {
                let (commands, errors) = resolve_document_with_details(path).map_err(|e| e.0)?;
                for command in commands { println!("{command}"); }
                if !errors.is_empty() {
                    for error in errors { eprintln!("{error}"); }
                    return Err("resolve-file failed".to_string());
                }
            }
            Ok(())
        }
        "resolve-project" => {
            let start_path = if args.len() < 2 { "." } else { args[1].as_str() };
            let path = find_sdkvers_path(start_path).map_err(|e| e.0)?;
            let (commands, errors) = resolve_document_with_details(&path).map_err(|e| e.0)?;
            for command in commands { println!("{command}"); }
            if !errors.is_empty() {
                for error in errors { eprintln!("{error}"); }
                return Err("resolve-project failed".to_string());
            }
            Ok(())
        }
        other => Err(format!("unknown debug subcommand: {other}")),
    }
}

fn run_internal(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None => Err("missing subcommand".to_string()),
        Some("extract") => run_extract(&args[1..]),
        Some(other) => Err(format!("unknown internal subcommand: {other}")),
    }
}

// ---------------------------------------------------------------------------
// `sdkvers` subcommand — shell function backend
// ---------------------------------------------------------------------------

fn run_fn(args: &[String]) -> Result<(), String> {
    let uuid = generate_fn_uuid();
    match args.first().map(String::as_str) {
        None => run_fn_resolve(&uuid),
        Some("bootstrap") => run_fn_bootstrap(&args[1..], &uuid),
        Some("selfupdate") => run_fn_selfupdate(&args[1..]),
        Some("help") => {
            let mut out = FnOutput::new();
            out.stdout = fn_help_text().to_string();
            out.write(&uuid);
            Ok(())
        }
        Some(other) => {
            eprintln!("{}", fn_help_text());
            Err(format!("unknown sdkvers subcommand: {other}"))
        }
    }
}

fn run_fn_resolve(uuid: &str) -> Result<(), String> {
    let path = find_sdkvers_path(".").map_err(|e| e.0)?;
    let (commands, errors) = resolve_document_with_details(&path).map_err(|e| e.0)?;

    if !errors.is_empty() {
        for error in &errors {
            eprintln!("{error}");
        }
        return Err(String::new());
    }

    let mut out = FnOutput::new();
    for cmd in &commands {
        out.eval.push_str(cmd);
        out.eval.push('\n');
    }
    out.write(uuid);
    Ok(())
}

fn run_fn_bootstrap(args: &[String], uuid: &str) -> Result<(), String> {
    let dir = match args.first().map(String::as_str) {
        None => ".",
        Some("--directory") => match args.get(1).map(String::as_str) {
            Some(d) => d,
            None => return Err("--directory requires a value".to_string()),
        },
        Some(other) => return Err(format!("unexpected argument: {other}")),
    };

    let target = Path::new(dir).join(".sdkvers");
    if target.exists() {
        return Err(format!(
            "'.sdkvers' already exists in {}",
            std::fs::canonicalize(dir)
                .unwrap_or_else(|_| std::path::PathBuf::from(dir))
                .display()
        ));
    }

    let content = bootstrap_sdkvers_content().map_err(|e| e.0)?;
    std::fs::write(&target, &content)
        .map_err(|e| format!("could not write .sdkvers: {e}"))?;

    let mut out = FnOutput::new();
    out.stdout = format!("wrote {}\n", target.display());
    out.write(uuid);
    Ok(())
}

// ---------------------------------------------------------------------------
// `selfupdate` subcommand
// ---------------------------------------------------------------------------

// selfupdate writes only to stderr and produces no stdout.  The shell
// function skips the extract/eval steps when stdout is empty, so the new
// binary is never asked to parse output from the old binary.  This avoids
// breakage when the internal protocol changes across versions.
fn run_fn_selfupdate(args: &[String]) -> Result<(), String> {
    let (check_only, force) = match args.first().map(String::as_str) {
        None => (false, false),
        Some("check") => (true, false),
        Some("force") => (false, true),
        Some(other) => return Err(format!("unexpected argument: {other}")),
    };

    let install_dir = resolve_install_dir()?;

    eprintln!("checking for updates...");
    let latest = fetch_latest_version()?;
    let current = env!("CARGO_PKG_VERSION");

    let latest_sv = parse_semver(&latest)
        .ok_or_else(|| format!("unexpected version format from GitHub: {latest}"))?;
    let current_sv = parse_semver(current)
        .ok_or_else(|| format!("unexpected current version format: {current}"))?;

    if latest_sv <= current_sv && !force {
        eprintln!("sdkvers {current} is up to date");
        return Ok(());
    }

    if latest_sv > current_sv {
        eprintln!("update available: {current} \u{2192} {latest}");
    }

    if check_only {
        eprintln!("run 'sdkvers selfupdate' to apply");
        return Ok(());
    }

    download_and_install(&latest, &install_dir)?;
    eprintln!("updated to {latest}");
    eprintln!("to complete the update, run: . {}", install_dir.join("sdkvers-init.sh").display());
    Ok(())
}

fn resolve_install_dir() -> Result<std::path::PathBuf, String> {
    // A: $SDKVERS_HOME if set, else ~/.sdkvers
    let home_base = std::env::var("SDKVERS_HOME").ok().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/.sdkvers")
    });
    let a = std::fs::canonicalize(&home_base)
        .map_err(|e| format!("cannot resolve install directory '{home_base}': {e}"))?;

    // B: directory containing the running binary
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot determine executable path: {e}"))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "executable has no parent directory".to_string())?;
    let b = std::fs::canonicalize(exe_dir)
        .map_err(|e| format!("cannot resolve executable directory: {e}"))?;

    if a != b {
        return Err(format!(
            "install directory mismatch: '{}' (from $SDKVERS_HOME/default) vs '{}' (from executable path)",
            a.display(),
            b.display()
        ));
    }

    Ok(a)
}

fn fetch_latest_version() -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "--silent",
            "--location",
            "--fail",
            "https://api.github.com/repos/jvasileff/sdkvers/releases/latest",
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !output.status.success() {
        return Err("failed to fetch release info from GitHub".to_string());
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let tag = extract_json_string_field(&body, "tag_name")
        .ok_or_else(|| "could not parse release version from GitHub response".to_string())?
        .to_owned();
    let version = tag.strip_prefix('v').unwrap_or(&tag).to_owned();
    Ok(version)
}

fn extract_json_string_field<'a>(json: &'a str, field: &str) -> Option<&'a str> {
    let needle = format!("\"{field}\"");
    let start = json.find(needle.as_str())?;
    let after_key = json[start + needle.len()..].trim_start();
    let after_colon = after_key.strip_prefix(':')?;
    let after_ws = after_colon.trim_start();
    let content = after_ws.strip_prefix('"')?;
    let end = content.find('"')?;
    Some(&content[..end])
}

fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    // Strip pre-release suffix (e.g. "-dev") and build metadata (e.g. "+build")
    let v = v.split('-').next().unwrap_or(v);
    let v = v.split('+').next().unwrap_or(v);
    let mut parts = v.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn download_and_install(version: &str, install_dir: &Path) -> Result<(), String> {
    let url = format!(
        "https://github.com/jvasileff/sdkvers/releases/download/v{version}/sdkvers.tar.gz"
    );

    let tmp_dir = create_temp_dir()?;

    let result = (|| {
        let tarball = tmp_dir.join("sdkvers.tar.gz");

        eprintln!("downloading sdkvers {version}...");
        let status = std::process::Command::new("curl")
            .args(["--silent", "--location", "--fail", "--output"])
            .arg(&tarball)
            .arg(&url)
            .status()
            .map_err(|e| format!("failed to run curl: {e}"))?;
        if !status.success() {
            return Err(format!("download failed: {url}"));
        }

        let status = std::process::Command::new("tar")
            .args(["-xzf"])
            .arg(&tarball)
            .arg("-C")
            .arg(&tmp_dir)
            .status()
            .map_err(|e| format!("failed to run tar: {e}"))?;
        if !status.success() {
            return Err("failed to extract update archive".to_string());
        }

        let extracted_dir = tmp_dir.join(format!("sdkvers-{version}"));
        if !extracted_dir.is_dir() {
            return Err(format!(
                "expected directory not found after extraction: {}",
                extracted_dir.display()
            ));
        }

        eprintln!("installing...");
        let entries = std::fs::read_dir(&extracted_dir)
            .map_err(|e| format!("failed to read extracted archive: {e}"))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to read archive entry: {e}"))?;
            let src = entry.path();
            let Some(filename) = src.file_name() else {
                continue;
            };
            let dst = install_dir.join(filename);
            // Write to a .new temp file then rename atomically so the running
            // binary's inode is preserved until the rename completes.
            let dst_tmp = install_dir.join(format!("{}.new", filename.to_string_lossy()));

            let perms = std::fs::metadata(&src)
                .map_err(|e| format!("failed to stat {}: {e}", src.display()))?
                .permissions();

            std::fs::copy(&src, &dst_tmp).map_err(|e| {
                format!("failed to copy {}: {e}", filename.to_string_lossy())
            })?;
            std::fs::set_permissions(&dst_tmp, perms).map_err(|e| {
                format!("failed to set permissions on {}: {e}", filename.to_string_lossy())
            })?;
            std::fs::rename(&dst_tmp, &dst).map_err(|e| {
                format!("failed to install {}: {e}", filename.to_string_lossy())
            })?;
        }

        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

fn create_temp_dir() -> Result<std::path::PathBuf, String> {
    use std::io::Read;
    let mut b = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|e| format!("failed to create temp directory: {e}"))?;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let dir = std::env::temp_dir().join(format!("sdkvers-update-{hex}"));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create temp directory: {e}"))?;
    Ok(dir)
}

// ---------------------------------------------------------------------------
// `extract` subcommand — split two-section output for the shell function
// ---------------------------------------------------------------------------

fn run_extract(args: &[String]) -> Result<(), String> {
    let section_idx = match args.first().map(String::as_str) {
        Some("eval") => 0usize,
        Some("stdout") => 1,
        Some(other) => {
            return Err(format!(
                "unknown section '{other}'; expected eval or stdout"
            ));
        }
        None => {
            return Err("usage: sdkvers-resolve extract <eval|stdout>".to_string());
        }
    };

    let mut input: Vec<u8> = Vec::new();
    std::io::stdin()
        .read_to_end(&mut input)
        .map_err(|e| format!("failed to read stdin: {e}"))?;

    // First line is the per-invocation UUID; the separator is \n{uuid}\n.
    let header_end = input
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| "malformed extract input: missing header line".to_string())?;
    let uuid = &input[..header_end];
    if !is_uuid(uuid) {
        return Err("malformed extract input: header line is not a UUID".to_string());
    }
    let rest = &input[header_end + 1..];

    // Section separator: \n{uuid}\n
    let mut sep = Vec::with_capacity(uuid.len() + 2);
    sep.push(b'\n');
    sep.extend_from_slice(uuid);
    sep.push(b'\n');

    // Closing delimiter: \n{uuid} (no trailing newline).  Strip it so the
    // last section isn't contaminated.
    let mut close = Vec::with_capacity(uuid.len() + 1);
    close.push(b'\n');
    close.extend_from_slice(uuid);

    let data = if rest.ends_with(&close) {
        &rest[..rest.len() - close.len()]
    } else {
        rest
    };

    let parts = split_bytes_n(data, &sep, 3);
    let qp_section: &[u8] = parts.get(section_idx).copied().unwrap_or(b"");

    if !qp_section.is_empty() {
        let decoded = qp_decode(qp_section)
            .map_err(|e| format!("QP decode error: {e}"))?;
        std::io::stdout()
            .write_all(&decoded)
            .map_err(|e| format!("failed to write stdout: {e}"))?;
    }

    Ok(())
}

fn split_bytes_n<'a>(data: &'a [u8], sep: &[u8], n: usize) -> Vec<&'a [u8]> {
    let mut parts: Vec<&[u8]> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + sep.len() <= data.len() {
        if parts.len() + 1 == n {
            break;
        }
        if data[i..].starts_with(sep) {
            parts.push(&data[start..i]);
            start = i + sep.len();
            i = start;
        } else {
            i += 1;
        }
    }
    parts.push(&data[start..]);
    parts
}

// ---------------------------------------------------------------------------
// Quoted-Printable codec (RFC 2045, LF line endings)
// ---------------------------------------------------------------------------

/// Encode arbitrary bytes as Quoted-Printable text.
///
/// - Printable ASCII (0x21–0x7E) except `=` passes through literally.
/// - Space and tab pass through literally, except when immediately before
///   `\n` or at end-of-data (trailing whitespace must be encoded per RFC 2045).
/// - `\n` is written as a literal newline (line break in QP text).
/// - Everything else, including `=` and `\0`, is encoded as `=XX`.
/// - Soft line breaks (`=\n`) are inserted to keep lines ≤ 76 characters.
fn qp_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut line_len: usize = 0;
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b == b'\n' {
            out.push('\n');
            line_len = 0;
            i += 1;
            continue;
        }
        let next = data.get(i + 1).copied();
        // Determine the encoded form of this byte.
        let encoded = if b == b'=' {
            format!("=3D")
        } else if (b == b' ' || b == b'\t') && (next == Some(b'\n') || next.is_none()) {
            format!("={:02X}", b)  // trailing whitespace must be encoded
        } else if b == b' ' || b == b'\t' || (0x21..=0x7E).contains(&b) {
            (b as char).to_string()
        } else {
            format!("={:02X}", b)
        };
        // Insert a soft line break if this token would exceed 76 chars
        // (the `=\n` itself counts as one character against the limit).
        if line_len + encoded.len() > 75 {
            out.push_str("=\n");
            line_len = 0;
        }
        out.push_str(&encoded);
        line_len += encoded.len();
        i += 1;
    }
    out
}

/// Decode Quoted-Printable bytes back to the original byte sequence.
///
/// Handles both LF and CRLF soft line breaks (`=\n` and `=\r\n`).
fn qp_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] != b'=' {
            out.push(data[i]);
            i += 1;
        } else {
            i += 1;
            if i >= data.len() {
                return Err("truncated = at end of input".to_string());
            }
            if data[i] == b'\n' {
                i += 1; // soft line break (LF)
            } else if data[i] == b'\r' && data.get(i + 1) == Some(&b'\n') {
                i += 2; // soft line break (CRLF)
            } else if i + 1 < data.len() {
                let hi = hex_val(data[i])
                    .ok_or_else(|| format!("invalid QP hex digit: 0x{:02X}", data[i]))?;
                let lo = hex_val(data[i + 1])
                    .ok_or_else(|| format!("invalid QP hex digit: 0x{:02X}", data[i + 1]))?;
                out.push((hi << 4) | lo);
                i += 2;
            } else {
                return Err("truncated =XX sequence at end of input".to_string());
            }
        }
    }
    Ok(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Return true if `bytes` is a well-formed UUID
/// (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, all hex, dashes at 8/13/18/23).
fn is_uuid(bytes: &[u8]) -> bool {
    const DASH: [usize; 4] = [8, 13, 18, 23];
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, &b)| {
            if DASH.contains(&i) { b == b'-' } else { b.is_ascii_hexdigit() }
        })
}
