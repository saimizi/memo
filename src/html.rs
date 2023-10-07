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
}
