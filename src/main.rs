use regex::Regex;

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
    memo::{FileName, MatchCondition, Memo, MemoEntry},
    std::{
        boxed::Box,
        collections::VecDeque,
        env,
        ffi::{CStr, CString},
        fmt::Display,
        fs,
        io::{self, Cursor},
        mem,
        process::Command,
        sync::atomic::{AtomicI32, Ordering},
    },
};

#[derive(Parser)]
#[command(author, version, about, long_about= None, help_template="
{before-help}{name} {version}
{author-with-newline}
{about-with-newline}
Environment variable
  EDITOR : editor used to create a new note (default: vim).
  BROWSER: browser used to display notes (default: w3m).

{usage-heading} {usage}

{all-args}{after-help}
")]
struct Cli {
    /// Root path to store memos, default is "$HOME/.memo/"
    #[arg(short, long)]
    path: Option<String>,

    /// Add text memo
    #[arg(short = 'a', long, conflicts_with = "add_html_memo")]
    add_text_memo: bool,

    /// Add html memo
    #[arg(short = 'A', long, conflicts_with = "add_text_memo")]
    add_html_memo: bool,

    /// Ignore case sensitivity
    #[arg(short = 'I', long, default_value_t = false)]
    ignore_case: bool,

    /// Delete notes in the search result.
    #[arg(short, long)]
    delete: bool,

    /// Match key as a word
    #[arg(short = 'W', long, default_value_t = false)]
    word: bool,

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

#[allow(unused)]
enum Op {
    Add,
    Sub,
    Mul,
    Nop,
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

    let condition = MatchCondition {
        ignore_case: cli.ignore_case,
        match_word: cli.word,
    };
    let memo = Memo::load(cli.path.as_deref())?;
    if memo.is_empty() {
        jinfo!("No memo.");
        return Ok(());
    }

    let mut result = String::new();
    let mut h1 = "Memo".to_string();
    let tag_entries = if let Some(tag) = &cli.tag {
        let tag = tag.trim();
        h1 = format!("Result for tag `{tag}`");
        memo.find(Some((tag, true, condition)))?
    } else {
        memo.new_search()
    };

    let args_entries = if let Some(keys) = &cli.args {
        let mut current = 0_usize;
        let mut search_queue = VecDeque::new();
        let mut op_queue = VecDeque::new();
        let keys = keys.trim_matches('+').trim_matches('-').trim_matches('*');

        loop {
            jdebug!(current = current, keys_len = keys.len());
            if current >= keys.len() {
                break;
            }

            let target = &keys[current..];
            let mut processed = false;
            for (pos, a) in target.as_bytes().iter().enumerate() {
                match a {
                    b'+' => {
                        let key = target[..pos].trim();

                        search_queue.push_back(memo.find(Some((key, false, condition)))?);
                        jdebug!("+ push for {key}\n{:?}", search_queue);
                        op_queue.push_back(Op::Add);
                        current += pos + 1;
                        processed = true;
                        break;
                    }
                    b'-' => {
                        let key = target[..pos].trim();
                        search_queue.push_back(memo.find(Some((key, false, condition)))?);
                        jdebug!("- push for {key}\n{:?}", search_queue);
                        op_queue.push_back(Op::Sub);
                        current += pos + 1;
                        processed = true;
                        break;
                    }
                    b'*' => {
                        let key = target[..pos].trim();
                        search_queue.push_back(memo.find(Some((key, false, condition)))?);
                        jdebug!("* push for {key}\n{:?}", search_queue);
                        op_queue.push_back(Op::Mul);
                        current += pos + 1;
                        processed = true;
                        break;
                    }
                    _ => {}
                }
            }

            if !processed {
                let key = target.trim();
                search_queue.push_back(memo.find(Some((key, false, condition)))?);
                jdebug!("push for {target}\n{:?}", search_queue);
                current += target.len();
            }
        }

        if !search_queue.is_empty() {
            let mut search = search_queue.pop_front().unwrap();

            while let Some(op) = op_queue.pop_front() {
                let new = search_queue.pop_front().unwrap();
                jdebug!("search before:\n{:?}", search);
                jdebug!("new:\n{:?}", new);
                match op {
                    Op::Add => search = (search + new)?,
                    Op::Sub => search = (search - new)?,
                    Op::Mul => search = (search * new)?,
                    Op::Nop => {}
                }
                jdebug!("search after:\n{:?}", search);
            }
            search
        } else {
            memo.new_search()
        }
    } else {
        memo.new_search()
    };

    let entries = if cli.tag.is_some() {
        if cli.args.is_some() {
            (tag_entries * args_entries)?
        } else {
            tag_entries
        }
    } else if cli.args.is_some() {
        args_entries
    } else {
        memo.find(None)?
    };

    if cli.delete {
        if !entries.is_empty() {
            for (i, entry) in entries.entries().into_iter().enumerate() {
                eprintln!(" {:<4} {}", i + 1, Html::clear_html_tags(entry.title()));
            }

            eprint!(
                "\nWhich to delete ( [y|yes|Y|Yes]=all | Index=like 1,2,3-5... | Other=cancel)?"
            );
            let mut selection = String::new();
            io::stdin().read_line(&mut selection).unwrap();

            if selection.is_empty() {
                return Ok(());
            }

            match selection.as_str().trim() {
                "y" | "yes" | "Y" | "Yes" => {
                    for entry in entries.entries().into_iter() {
                        jdebug!("Remove {}\n({})", entry.full_path(), entry.title());
                        if let Err(e) = fs::remove_file(entry.full_path()) {
                            jerror!("Failed to remove {}: {:?}", entry.full_path(), e);
                        }
                    }
                }
                patten => {
                    let mut index: Vec<usize> = vec![];
                    let patten = patten.replace(' ', "");
                    let re = Regex::new("(([0-9]+-[0-9]+)|([0-9]+))").unwrap();

                    for it in re.find_iter(&patten) {
                        let number = it.as_str();

                        if number.contains('-') {
                            if let Some(pos) = number.as_bytes().iter().position(|&a| a == b'-') {
                                let (a, b) = number.split_at(pos);
                                let a = a.parse::<usize>().unwrap();
                                let b = b.trim_matches('-').parse::<usize>().unwrap();

                                let mut start = a;
                                let mut end = b;

                                if a > b {
                                    start = b;
                                    end = a;
                                }

                                for i in start..=end {
                                    if !index.iter().any(|&a| a == i) {
                                        index.push(i);
                                    }
                                }
                            }
                        } else {
                            index.push(number.parse::<usize>().unwrap());
                        }
                    }

                    for (i, entry) in entries.entries().into_iter().enumerate() {
                        if index.iter().any(|&a| a == i + 1) {
                            jdebug!("Remove {}\n({})", entry.full_path(), entry.title());
                            if let Err(e) = fs::remove_file(entry.full_path()) {
                                jerror!("Failed to remove {}: {:?}", entry.full_path(), e);
                            }
                        }
                    }
                }
            }
        } else {
            jinfo!("No memo to delete.")
        }
        return Ok(());
    }

    if !entries.is_empty() {
        result.push_str(&Html::h1(&format!("{h1} ({})", entries.entries().len())));

        let entries: Vec<String> = entries
            .entries()
            .iter()
            .map(|&a| {
                let fix = Html::clear_html_tags(a.title());
                let mut s = Html::link(&fix, a.full_path());
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

            let browser = env::var("BROWSER").unwrap_or("w3m".to_owned());
            let mut args = vec![];
            if &browser == "w3m" {
                args.push("-num");
                args.push("-T");
                args.push("text/html");
            }

            args.push(&output);
            let mut handle = Command::new(browser).args(args).spawn().map_err(|e| {
                Report::new(MemoError::Unexpected)
                    .attach_printable(format!("Failed to execute w3m: {e}"))
            })?;

            handle.wait().map_err(|e| {
                Report::new(MemoError::Unexpected).attach_printable(format!("w3m failed: {e}"))
            })?;
        } else {
            jinfo!("No memo.");
        }
    } else {
        jinfo!("No memo.");
    }

    Ok(())
}
