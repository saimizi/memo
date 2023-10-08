mod error;
mod html;
mod memo;

#[allow(unused)]
use {
    chrono::Local,
    clap::Parser,
    error::MemoError,
    error_stack::{Report, Result, ResultExt},
    html::Html,
    jlogger_tracing::{
        jdebug, jerror, jinfo, jtrace, jwarn, JloggerBuilder, LevelFilter, LogTimeFormat,
    },
    memo::{FileName, Memo, MemoEntry},
    std::{
        boxed::Box,
        ffi::{CStr, CString},
        fmt::Display,
        fs, mem,
        process::Command,
        sync::atomic::{AtomicI32, Ordering},
    },
};

#[derive(Parser)]
#[command(author, version, about, long_about= None)]
struct Cli {
    /// Memo path
    #[arg(short, long)]
    path: Option<String>,

    /// Add text memo
    #[arg(short = 'a', long, conflicts_with = "add_html_memo")]
    add_text_memo: bool,

    /// Add html memo
    #[arg(short = 'A', long, conflicts_with = "add_text_memo")]
    add_html_memo: bool,

    /// Search the memo with a tag of "TAG"
    #[arg(short, long)]
    tag: Option<String>,

    /// Log file
    #[arg(short, long)]
    log: Option<String>,

    #[arg(short, long, action=clap::ArgAction::Count)]
    verbose: u8,

    /// Keyword used to search memo
    args: Option<String>,
}

fn main() -> Result<(), MemoError> {
    let cli = Cli::parse();

    let level = match cli.verbose {
        1 => LevelFilter::DEBUG,
        2 => LevelFilter::TRACE,
        _ => LevelFilter::INFO,
    };

    if let Some(log) = cli.log.as_deref() {
        JloggerBuilder::new()
            .log_console(false)
            .log_file(Some((log, false)))
            .max_level(level)
            .log_time(LogTimeFormat::TimeNone)
            .build();
    } else {
        JloggerBuilder::new()
            .max_level(level)
            .log_time(LogTimeFormat::TimeNone)
            .build();
    }

    if cli.add_text_memo || cli.add_html_memo {
        Memo::create(cli.path.as_deref(), cli.add_html_memo)?;
        return Ok(());
    }

    let memo = Memo::load(cli.path.as_deref())?;

    if memo.is_empty() {
        jinfo!("No memo.");
        return Ok(());
    }

    let mut result = String::new();

    let mut h1 = "Memo".to_string();

    let entries = {
        if let Some(tag) = &cli.tag {
            h1 = format!("Result for tag `{tag}`");
            memo.find_else(|entry: &MemoEntry| -> bool {
                if let Some(key) = &cli.args {
                    entry.match_tag(tag) && entry.match_any(key)
                } else {
                    entry.match_tag(tag)
                }
            })?
        } else if let Some(key) = &cli.args {
            h1 = format!("Result for key `{key}`");
            memo.find(Some((key, false)))?
        } else {
            memo.find(None)?
        }
    };

    result.push_str(&Html::h1(&format!("{h1} ({})", entries.entries().len())));

    let entries: Vec<String> = entries
        .entries()
        .iter()
        .map(|&a| {
            let mut s = Html::link(a.title(), a.full_path());
            s.push('\n');
            s.push_str(&format!("tags: {}", a.tags()));
            s.push('\n');
            s.push_str(&format!("created at: {}", a.create_time()));
            s
        })
        .collect();

    if !entries.is_empty() {
        result.push_str(&Html::list(entries.iter().map(|a| a.as_str()).collect()));

        let output = format!("{}/index.html", memo.root());

        let _ = fs::remove_file(&output);
        fs::write(&output, result).map_err(|e| {
            Report::new(MemoError::IOError)
                .attach_printable(format!("Failed to write result to {output}: {e}"))
        })?;

        let mut handle = Command::new("w3m")
            .arg("-num")
            .args(["-T", "text/html"])
            .arg(&output)
            .spawn()
            .map_err(|e| {
                Report::new(MemoError::Unexpected)
                    .attach_printable(format!("Failed to execute w3m: {e}"))
            })?;

        handle.wait().map_err(|e| {
            Report::new(MemoError::Unexpected).attach_printable(format!("w3m failed: {e}"))
        })?;
    } else {
        jinfo!("No memo.")
    }

    Ok(())
}
