//! `cargo-supabase` — code generation companion for `rust_supabase_sdk`.
//!
//! Run as `cargo supabase gen types --url $SUPABASE_URL --apikey $SUPABASE_API_KEY`.
//!
//! Fetches PostgREST's OpenAPI document, then emits a Rust module of row
//! structs and `Row` impls suitable for `client.from_row::<T>()`.

mod codegen;

use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use codegen::{Options, OpenApi};

#[derive(Debug, Default)]
struct CliArgs {
    url: Option<String>,
    apikey: Option<String>,
    schema: Option<String>,
    output: Option<PathBuf>,
    only: Vec<String>,
    exclude: Vec<String>,
    no_chrono: bool,
    uuid: bool,
    help: bool,
}

const USAGE: &str = "\
cargo-supabase — codegen for rust_supabase_sdk

USAGE:
    cargo supabase gen types [OPTIONS]

OPTIONS:
    --url <URL>              Supabase project URL (or env SUPABASE_URL)
    --apikey <KEY>           Anon / service-role key  (or env SUPABASE_API_KEY)
    --schema <NAME>          Schema label baked into generated docs [default: public]
    --output <PATH>          Write to file instead of stdout
    --only <TABLE>           Allow-list (repeatable)
    --exclude <TABLE>        Deny-list (repeatable)
    --no-chrono              Emit timestamps as `String` instead of `chrono::*`
    --uuid                   Emit uuid-format columns as `uuid::Uuid`
    -h, --help               Print this message
";

fn main() -> Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // Allow both `cargo-supabase gen types ...` and `cargo supabase gen types ...`
    // — when invoked as a cargo subcommand, the first arg is the subcommand name
    // ("supabase") which we strip.
    let mut iter = raw.iter().peekable();
    if iter.peek().map(|s| s.as_str()) == Some("supabase") {
        iter.next();
    }
    let argv: Vec<String> = iter.cloned().collect();

    match argv.first().map(String::as_str) {
        Some("gen") => match argv.get(1).map(String::as_str) {
            Some("types") => run_gen_types(&argv[2..]),
            Some(other) => bail!("unknown `gen` target: {other}\n\n{USAGE}"),
            None => bail!("missing subcommand for `gen`\n\n{USAGE}"),
        },
        Some("-h") | Some("--help") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => bail!("unknown subcommand: {other}\n\n{USAGE}"),
    }
}

fn run_gen_types(argv: &[String]) -> Result<()> {
    let args = parse_args(argv)?;
    if args.help {
        println!("{USAGE}");
        return Ok(());
    }

    let url = args
        .url
        .or_else(|| std::env::var("SUPABASE_URL").ok())
        .context("--url not provided and SUPABASE_URL not set")?;
    let apikey = args
        .apikey
        .or_else(|| std::env::var("SUPABASE_API_KEY").ok())
        .or_else(|| std::env::var("SUPABASE_SERVICE_ROLE_KEY").ok())
        .context("--apikey not provided and SUPABASE_API_KEY not set")?;
    let schema = args.schema.unwrap_or_else(|| "public".to_string());

    let opts = Options {
        schema: schema.clone(),
        only: args.only,
        exclude: args.exclude,
        chrono: !args.no_chrono,
        uuid_type: args.uuid,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;
    let api: OpenApi = rt.block_on(fetch_openapi(&url, &apikey, &schema))?;

    let body = codegen::emit(&api, &opts);

    match args.output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create directory {}", parent.display())
                    })?;
                }
            }
            fs::write(&path, &body)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("wrote {} bytes to {}", body.len(), path.display());
        }
        None => {
            print!("{body}");
        }
    }
    Ok(())
}

async fn fetch_openapi(url: &str, apikey: &str, schema: &str) -> Result<OpenApi> {
    let endpoint = format!("{}/rest/v1/", url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .build()
        .context("failed to build HTTP client")?;
    let resp = client
        .get(&endpoint)
        .header("apikey", apikey)
        .header("Authorization", format!("Bearer {apikey}"))
        .header("Accept-Profile", schema)
        .header("Accept", "application/openapi+json,application/json;q=0.9")
        .send()
        .await
        .with_context(|| format!("GET {endpoint}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .context("reading PostgREST response body")?;
    if !status.is_success() {
        bail!("PostgREST returned {status}: {body}");
    }
    serde_json::from_str(&body).context("parsing PostgREST OpenAPI document")
}

fn parse_args(argv: &[String]) -> Result<CliArgs> {
    let mut args = CliArgs::default();
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        let mut take_next = || -> Result<String> {
            i += 1;
            argv.get(i)
                .cloned()
                .with_context(|| format!("flag `{a}` requires a value"))
        };
        match a.as_str() {
            "--url" => args.url = Some(take_next()?),
            "--apikey" => args.apikey = Some(take_next()?),
            "--schema" => args.schema = Some(take_next()?),
            "--output" | "-o" => args.output = Some(PathBuf::from(take_next()?)),
            "--only" => args.only.push(take_next()?),
            "--exclude" => args.exclude.push(take_next()?),
            "--no-chrono" => args.no_chrono = true,
            "--uuid" => args.uuid = true,
            "-h" | "--help" => args.help = true,
            other => bail!("unknown flag: {other}\n\n{USAGE}"),
        }
        i += 1;
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_collects_repeats_and_flags() {
        let argv = [
            "--url", "https://x.supabase.co",
            "--apikey", "k",
            "--only", "a", "--only", "b",
            "--exclude", "c",
            "--no-chrono",
            "--uuid",
            "-o", "out.rs",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

        let args = parse_args(&argv).unwrap();
        assert_eq!(args.url.as_deref(), Some("https://x.supabase.co"));
        assert_eq!(args.apikey.as_deref(), Some("k"));
        assert_eq!(args.only, vec!["a", "b"]);
        assert_eq!(args.exclude, vec!["c"]);
        assert!(args.no_chrono);
        assert!(args.uuid);
        assert_eq!(args.output, Some(PathBuf::from("out.rs")));
    }

    #[test]
    fn parse_args_unknown_flag_errors() {
        let argv = vec!["--bogus".to_string()];
        assert!(parse_args(&argv).is_err());
    }

    #[test]
    fn parse_args_schema_flag() {
        let argv = vec!["--schema".to_string(), "private".to_string()];
        let args = parse_args(&argv).unwrap();
        assert_eq!(args.schema.as_deref(), Some("private"));
    }

    #[test]
    fn parse_args_help_short_and_long() {
        let h1 = parse_args(&["-h".to_string()]).unwrap();
        assert!(h1.help);
        let h2 = parse_args(&["--help".to_string()]).unwrap();
        assert!(h2.help);
    }

    #[test]
    fn parse_args_output_long_form() {
        let argv = vec!["--output".to_string(), "types.rs".to_string()];
        let args = parse_args(&argv).unwrap();
        assert_eq!(args.output, Some(PathBuf::from("types.rs")));
    }

    #[test]
    fn parse_args_empty_argv_returns_defaults() {
        let args = parse_args(&[]).unwrap();
        assert!(args.url.is_none());
        assert!(args.apikey.is_none());
        assert!(args.only.is_empty());
        assert!(args.exclude.is_empty());
        assert!(!args.no_chrono);
        assert!(!args.uuid);
        assert!(!args.help);
        assert!(args.output.is_none());
    }

    #[test]
    fn parse_args_flag_missing_value_errors() {
        // A flag that requires a value but has none at end-of-args.
        let argv = vec!["--url".to_string()];
        let err = parse_args(&argv).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--url") && msg.contains("requires a value"), "msg={msg}");
    }

    #[test]
    fn parse_args_apikey_alone() {
        let argv = vec!["--apikey".to_string(), "secret".to_string()];
        let args = parse_args(&argv).unwrap();
        assert_eq!(args.apikey.as_deref(), Some("secret"));
    }
}
