#[allow(unused)]
use {
    super::{
        error::MemoError,
        memo::{FileName, Memo, MemoEntry},
    },
    chrono::Local,
    clap::Parser,
    error_stack::{Report, Result, ResultExt},
    jlogger_tracing::{
        jdebug, jerror, jinfo, jtrace, jwarn, JloggerBuilder, LevelFilter, LogTimeFormat,
    },
    regex::Regex,
    std::{
        boxed::Box,
        ffi::{CStr, CString},
        fmt::Display,
        fs, mem,
        sync::atomic::{AtomicI32, Ordering},
    },
};

pub struct Html;

impl Html {
    pub fn h1(s: &str) -> String {
        format!("<H1>{s}</H1>")
    }

    pub fn link(title: &str, link: &str) -> String {
        format!("<a href={link}>{title}</a>")
    }

    pub fn list(entries: Vec<&str>) -> String {
        let mut list = String::new();

        list.push_str("<ul>\n");

        for l in entries {
            let l = l.replace('\n', "<br>");

            list.push_str(&format!("<li>{l}</li>\n"));
        }

        list.push_str("</ul>\n");

        list
    }

    pub fn clear_html_tags(orig: &str) -> String {
        let clear = |re: Regex, orig: &str, replace: &str| -> String {
            let mut matched = vec![];
            for caps in re.captures_iter(orig) {
                let m = caps.get(0).unwrap();
                matched.push(m.as_str());
            }

            let mut fix = orig.to_string();
            for s in matched {
                fix = fix.replace(s, replace).to_owned();
            }

            fix.trim().to_owned()
        };

        let re = Regex::new(r"<[a-z|A-Z|0-9|/|:|_|^|-|%|&| |.|=]+>").unwrap();
        let fix = clear(re, orig, "");

        let re = Regex::new(" +").unwrap();
        clear(re, &fix, " ")
    }
}
