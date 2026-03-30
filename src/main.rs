use sdkvers::{
    ConfigLineParser, Resolver, VersionParser, dump_config_line, dump_document, dump_sdk_list,
    dump_version, dump_version_expr, find_sdkvers_path, load_local_sdk_list, parse_document,
    parse_sdk_list, read_utf8_file, resolve_document_with_details, run_sdk_list, self_test,
};
use std::process::ExitCode;

fn print_usage() {
    eprintln!(
        "usage: sdkvers-resolve <command> [args...]"
    );
    eprintln!();
    eprintln!("resolve commands:");
    eprintln!("  resolve-project [dir]               resolve .sdkvers from dir (default: .)");
    eprintln!("  resolve-file <path>                 resolve a specific .sdkvers file");
    eprintln!("  resolve-line <line>...              resolve one config line against local SDKs");
    eprintln!();
    eprintln!("parse commands (for inspection and testing):");
    eprintln!("  parse-version <version>...          parse and dump a version string");
    eprintln!("  parse-expr <expr>...                parse and dump a version expression");
    eprintln!("  parse-line <line>...                parse and dump a .sdkvers config line");
    eprintln!("  parse-file <path>...                parse and dump a .sdkvers file");
    eprintln!("  parse-sdkfile <candidate> <path>... parse sdk list output from a file");
    eprintln!("  parse-sdklist <candidate>...        run sdk list and parse its output");
    eprintln!();
    eprintln!("other:");
    eprintln!("  self-test                           run the built-in test suite");
    eprintln!("  --version                           print version");
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return Err("missing command".to_string());
    }

    if args[0] == "--version" {
        println!("sdkvers-resolve {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let known_commands = [
        "parse-version",
        "parse-expr",
        "parse-line",
        "parse-file",
        "parse-sdkfile",
        "parse-sdklist",
        "resolve-line",
        "resolve-file",
        "resolve-project",
        "self-test",
    ];

    let command = args[0].as_str();
    if !known_commands.contains(&command) {
        print_usage();
        return Err(format!("unknown command: {command}"));
    }

    if command == "self-test" {
        self_test(|name| eprintln!("  {name}")).map_err(|e| e.0)?;
        eprintln!("self-tests passed");
        return Ok(());
    }

    if command != "resolve-project" && args.len() < 2 {
        print_usage();
        return Err("missing arguments".to_string());
    }

    match command {
        "parse-version" => {
            for (i, value) in args[1..].iter().enumerate() {
                if i > 0 {
                    println!();
                }
                let parsed = VersionParser::new(value).parse_version().map_err(|e| e.0)?;
                print!("{}", dump_version(&parsed));
            }
        }
        "parse-expr" => {
            for (i, value) in args[1..].iter().enumerate() {
                if i > 0 {
                    println!();
                }
                let parsed = VersionParser::new(value)
                    .parse_version_expr()
                    .map_err(|e| e.0)?;
                print!("{}", dump_version_expr(&parsed));
            }
        }
        "parse-line" => {
            for (i, value) in args[1..].iter().enumerate() {
                if i > 0 {
                    println!();
                }
                let parsed = ConfigLineParser::new(value, 1).parse_line().map_err(|e| e.0)?;
                print!("{}", dump_config_line(&parsed));
            }
        }
        "parse-file" => {
            for (i, path) in args[1..].iter().enumerate() {
                if i > 0 {
                    println!();
                }
                let document = parse_document(&read_utf8_file(path).map_err(|e| e.0)?);
                print!("{}", dump_document(&document));
            }
        }
        "parse-sdkfile" => {
            let inputs = &args[1..];
            if inputs.len() < 2 || inputs.len() % 2 != 0 {
                return Err(
                    "usage: sdkvers-resolve parse-sdkfile <candidate> <path> [<candidate> <path> ...]"
                        .to_string(),
                );
            }
            let mut idx = 0;
            let mut first = true;
            while idx < inputs.len() {
                if !first {
                    println!();
                }
                first = false;
                let candidate = &inputs[idx];
                let path = &inputs[idx + 1];
                let node = parse_sdk_list(candidate, &read_utf8_file(path).map_err(|e| e.0)?);
                print!("{}", dump_sdk_list(&node));
                idx += 2;
            }
        }
        "parse-sdklist" => {
            for (i, candidate) in args[1..].iter().enumerate() {
                if i > 0 {
                    println!();
                }
                let text = run_sdk_list(candidate).map_err(|e| e.0)?;
                let node = parse_sdk_list(candidate, &text);
                print!("{}", dump_sdk_list(&node));
            }
        }
        "resolve-line" => {
            let resolver = Resolver;
            for (i, line_text) in args[1..].iter().enumerate() {
                if i > 0 {
                    println!();
                }
                let line = ConfigLineParser::new(line_text, 1).parse_line().map_err(|e| e.0)?;
                let sdk = load_local_sdk_list(&line.candidate).map_err(|e| e.0)?;
                let row = resolver.resolve_line(&line, &sdk).map_err(|e| e.0)?;
                println!("sdk use {} {}", row.candidate, row.target);
            }
        }
        "resolve-file" => {
            for path in &args[1..] {
                let (commands, errors) = resolve_document_with_details(path).map_err(|e| e.0)?;
                for command in commands {
                    println!("{command}");
                }
                if !errors.is_empty() {
                    for error in errors {
                        eprintln!("{error}");
                    }
                    return Err("resolve-file failed".to_string());
                }
            }
        }
        "resolve-project" => {
            let start_path = if args.len() < 2 { "." } else { args[1].as_str() };
            let path = find_sdkvers_path(start_path).map_err(|e| e.0)?;
            let (commands, errors) = resolve_document_with_details(&path).map_err(|e| e.0)?;
            for command in commands {
                println!("{command}");
            }
            if !errors.is_empty() {
                for error in errors {
                    eprintln!("{error}");
                }
                return Err("resolve-project failed".to_string());
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}
