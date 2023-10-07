#[allow(unused)]
use {
    clap::Parser,
    error_stack::{Report, Result, ResultExt},
    jlogger_tracing::{
        jdebug, jerror, jinfo, jtrace, jwarn, JloggerBuilder, LevelFilter, LogTimeFormat,
    },
    std::{
        boxed::Box,
        error::Error,
        ffi::{CStr, CString},
        fmt::Display,
        fs, mem,
        sync::atomic::{AtomicI32, Ordering},
    },
};

#[derive(Debug)]
pub enum MemoError {
    InvalidValue,
    IOError,
    Unexpected,
}

impl Display for MemoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            MemoError::InvalidValue => "Invalid value",
            MemoError::IOError => "IO error",
            MemoError::Unexpected => "Unexpected error",
        };

        write!(f, "{}", msg)
    }
}

impl Error for MemoError {}
