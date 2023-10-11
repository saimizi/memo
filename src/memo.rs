#[allow(unused)]
use {
    super::error::MemoError,
    chrono::{Datelike, Local, Timelike},
    clap::Parser,
    error_stack::{Report, Result, ResultExt},
    jlogger_tracing::{
        jdebug, jerror, jinfo, jtrace, jwarn, JloggerBuilder, LevelFilter, LogTimeFormat,
    },
    regex::Regex,
    std::{
        boxed::Box,
        env,
        ffi::{CStr, CString},
        fmt::{self, Debug, Display},
        fs::{self, DirEntry},
        io::{BufRead, BufReader, BufWriter, Read, Write},
        mem,
        ops::{Add, Mul, Sub},
        path::{Path, PathBuf},
        process::Command,
        rc::Rc,
        sync::atomic::{AtomicI32, Ordering},
    },
};

#[derive(Debug, Clone, Copy)]
pub struct MatchCondition {
    pub ignore_case: bool,
    pub match_word: bool,
}

#[derive(Debug)]
pub struct FileName {
    year: String,
    month: String,
    day: String,
    hour: String,
    minute: String,
    second: String,
    suffix: String,
}

impl FileName {
    pub fn create(html: bool) -> Self {
        let now = Local::now();
        let mut suffix = String::from("txt");
        if html {
            suffix = String::from("html");
        }

        Self {
            year: now.year().to_string(),
            month: now.month().to_string(),
            day: now.day().to_string(),
            hour: now.hour().to_string(),
            minute: now.minute().to_string(),
            second: now.second().to_string(),
            suffix,
        }
    }

    pub fn create_time(&self) -> String {
        format!(
            "{:0>4}/{:0>2}/{:0>2} {:0>2}:{:0>2}:{:0>2}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    pub fn file_name(&self) -> String {
        format!(
            "{:0>4}_{:0>2}_{:0>2}_{:0>2}_{:0>2}_{:0>2}.{}",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.suffix,
        )
    }

    pub fn from_file_name(name: &str) -> Result<Self, MemoError> {
        let path = Path::new(name);
        let create_time = path
            .file_stem()
            .ok_or(
                Report::new(MemoError::InvalidValue)
                    .attach_printable(format!("Invalid file name {name}")),
            )?
            .to_str()
            .unwrap();

        let suffix = path
            .extension()
            .map(|a| a.to_str().unwrap())
            .unwrap_or("txt");

        if suffix != "txt" && suffix != "html" {
            jerror!("Invalid suffix for {name}");
            return Err(Report::new(MemoError::InvalidValue)).attach_printable("Invalid suffix");
        }

        let suffix = suffix.to_owned();
        let mut s = create_time.split('_').collect::<Vec<&str>>();

        let mut field = |f: &str| -> Result<String, MemoError> {
            Ok(s.pop()
                .ok_or(
                    Report::new(MemoError::InvalidValue)
                        .attach_printable(format!("No {f} entry in file name")),
                )?
                .to_string())
        };

        let second = field("second")?;
        let minute = field("minute")?;
        let hour = field("hour")?;
        let day = field("day")?;
        let month = field("month")?;
        let year = field("year")?;

        Ok(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            suffix,
        })
    }
}

impl PartialEq for FileName {
    fn eq(&self, other: &Self) -> bool {
        (self.year == other.year)
            && (self.month == other.month)
            && (self.day == other.day)
            && (self.hour == other.hour)
            && (self.minute == other.minute)
            && (self.second == other.second)
            && (self.suffix == other.suffix)
    }
}

pub struct MemoEntry {
    title: String,
    body: String,
    tags: Vec<String>,
    name: FileName,
    full_path: String,
}

impl Debug for MemoEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.full_path)
    }
}

#[allow(unused)]
impl MemoEntry {
    pub fn load(file: &str) -> Result<MemoEntry, MemoError> {
        let path = Path::new(file);
        let file_name = path
            .file_name()
            .ok_or(Report::new(MemoError::InvalidValue))
            .attach_printable(format!("Invalid memo file name {file}"))?
            .to_str()
            .ok_or(Report::new(MemoError::InvalidValue))
            .attach_printable(format!("Invalid memo file name {file}"))?;

        let name = FileName::from_file_name(file_name)?;

        let f = fs::File::open(file).map_err(|e| {
            Report::new(MemoError::IOError).attach_printable(format!("Failed to open {file} : {e}"))
        })?;

        let full_path = fs::canonicalize(path)
            .map_err(|e| {
                Report::new(MemoError::Unexpected)
                    .attach_printable(format!("Failed to retrieve absolute path for {file} : {e}"))
            })?
            .as_path()
            .to_str()
            .unwrap()
            .to_owned();
        let mut reader = BufReader::new(f);
        let mut title = String::new();
        let mut body = String::new();
        let mut tags = vec![];

        loop {
            let mut line = String::new();

            let size = reader.read_line(&mut line).map_err(|e| {
                Report::new(MemoError::IOError)
                    .attach_printable(format!("Failed to open {file} : {e}"))
            })?;

            if size == 0 {
                break;
            }

            if title.is_empty() {
                title.push_str(&line);
                title = title.trim_end_matches('\n').to_owned();

                let re = Regex::new(r"(\[[a-z|A-Z|0-9|_|-]+\])").unwrap();
                tags = re
                    .find_iter(&title)
                    .map(|m| m.as_str().to_owned())
                    .collect();

                /*
                let re = Regex::new(r"(?<tag>\[[a-z|A-Z|0-9|_|-]+\])").unwrap();
                let mut it = re.captures_iter(&title);

                jdebug!(title = title);
                while let Some(t) = it.next() {
                    let s = &t["tag"];
                    jdebug!(s = s);
                    tags.push(s.to_owned());
                }
                */
            } else {
                body.push_str(&line);
            }
        }

        if title.trim().is_empty() {
            return Err(
                Report::new(MemoError::Unexpected).attach_printable(format!("{file} is empty"))
            );
        }

        Ok(Self {
            title,
            body,
            tags,
            name,
            full_path,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn tags(&self) -> String {
        let mut tag = String::new();

        self.tags
            .iter()
            .for_each(|a| tag.push_str(&format!("{a} ")));

        tag.trim().to_owned()
    }

    pub fn create_time(&self) -> String {
        self.name.create_time()
    }

    pub fn match_tag(&self, tag: &str, condition: MatchCondition) -> bool {
        let key = if condition.match_word {
            format!(r"\b{tag}\b")
        } else {
            tag.to_owned()
        };

        let re = regex::RegexBuilder::new(&key)
            .case_insensitive(condition.ignore_case)
            .build()
            .unwrap();

        self.tags
            .iter()
            .any(|a| re.is_match(a.trim_matches('[').trim_matches(']')))
    }

    pub fn match_content(&self, key: &str, condition: MatchCondition) -> bool {
        let key = if condition.match_word {
            format!(r"\b{key}\b")
        } else {
            key.to_owned()
        };

        let re = regex::RegexBuilder::new(&key)
            .case_insensitive(condition.ignore_case)
            .build()
            .unwrap();

        re.is_match(&self.title) || re.is_match(&self.body)
    }

    pub fn match_any(&self, key: &str, condition: MatchCondition) -> bool {
        self.match_tag(key, condition) || self.match_content(key, condition)
    }

    pub fn full_path(&self) -> &str {
        &self.full_path
    }
}

impl PartialEq for MemoEntry {
    fn eq(&self, other: &Self) -> bool {
        self.full_path == other.full_path
    }
}

pub struct Memo {
    entries: Vec<MemoEntry>,
    root: String,
}

#[allow(unused)]
impl Memo {
    fn setup_root(root_path: Option<&str>) -> Result<(String, String), MemoError> {
        let mut root = format!("{}/.memo", env!("HOME"));

        if let Some(r) = root_path {
            root = r.to_string();
        }

        let r_path = Path::new(&root);

        if !r_path.exists() {
            fs::create_dir_all(r_path).map_err(|e| {
                Report::new(MemoError::Unexpected)
                    .attach_printable(format!("Failed to create root path {root}:{e}"))
            })?;
        }

        if !r_path.is_dir() {
            return Err(Report::new(MemoError::InvalidValue))
                .attach_printable(format!("{root} is not a directory."));
        }

        let memo_dir = format!("{root}/memo");
        let m_path = Path::new(&memo_dir);
        if !m_path.exists() {
            fs::create_dir_all(m_path).map_err(|e| {
                Report::new(MemoError::Unexpected)
                    .attach_printable(format!("Failed to create memo path {memo_dir}: {e}"))
            })?;
        }

        Ok((root, memo_dir))
    }

    pub fn load(root_path: Option<&str>) -> Result<Self, MemoError> {
        let mut entries = Vec::new();
        let (root, memo_dir) = Memo::setup_root(root_path)?;

        let m_path = Path::new(&memo_dir);
        let mut it = fs::read_dir(m_path).map_err(|e| {
            Report::new(MemoError::IOError)
                .attach_printable(format!("Failed to read dir {memo_dir} : {e}"))
        })?;

        while let Some(Ok(entry)) = it.next() {
            let entry = entry.path();
            let p = entry.as_path();

            if let Some(f) = p.to_str() {
                if !p.is_file() {
                    jwarn!("Skip {} which is not a file.", f);
                    continue;
                }

                match MemoEntry::load(f) {
                    Ok(m) => {
                        entries.push(m);
                    }
                    Err(e) => {
                        jwarn!("Failed to load {f}, remove it:\n{:?}.", e);
                        if let Err(e) = fs::remove_file(f) {
                            jwarn!("Failed to remove {f}:\n{:?}", e);
                        }
                    }
                }
            }
        }

        Ok(Self { entries, root })
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn new_search(&self) -> MemoSearch {
        MemoSearch {
            entries: vec![],
            root: &self.root,
        }
    }

    pub fn find(
        &self,
        key_pair: Option<(&str, bool, MatchCondition)>,
    ) -> Result<MemoSearch, MemoError> {
        let mut result = vec![];
        if let Some((key, is_tag, condition)) = key_pair {
            for entry in &self.entries {
                if is_tag {
                    if entry.match_tag(key, condition) {
                        result.push(entry);
                    }
                } else if entry.match_any(key, condition) {
                    result.push(entry);
                }
            }
        } else {
            for entry in &self.entries {
                result.push(entry);
            }
        }

        Ok(MemoSearch {
            entries: result,
            root: &self.root,
        })
    }

    pub fn find_else<F>(&self, cb: F) -> Result<MemoSearch, MemoError>
    where
        F: Fn(&MemoEntry) -> bool,
    {
        let mut result = vec![];

        for entry in &self.entries {
            if cb(entry) {
                result.push(entry);
            }
        }

        Ok(MemoSearch {
            entries: result,
            root: &self.root,
        })
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn create(root_path: Option<&str>, is_html: bool) -> Result<(), MemoError> {
        let (_root, memo_dir) = Memo::setup_root(root_path)?;
        let file_name = FileName::create(is_html);

        let output = format!("{memo_dir}/{}", file_name.file_name());
        let editor = env::var("EDITOR").unwrap_or("vim".to_owned());
        let mut handle = Command::new(editor).arg(&output).spawn().map_err(|e| {
            Report::new(MemoError::Unexpected)
                .attach_printable(format!("Failed to execute vim: {e}"))
        })?;

        handle.wait().map_err(|e| {
            Report::new(MemoError::Unexpected).attach_printable(format!("vim failed: {e}"))
        })?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct MemoSearch<'a> {
    entries: Vec<&'a MemoEntry>,
    root: &'a str,
}

impl<'a> MemoSearch<'a> {
    #[allow(unused)]
    pub fn find(&self, key_pair: Option<(&str, bool, MatchCondition)>) -> Result<Self, MemoError> {
        let mut result = vec![];
        if let Some((key, is_tag, condition)) = key_pair {
            for &entry in &self.entries {
                if is_tag {
                    if entry.match_tag(key, condition) {
                        result.push(entry);
                    }
                } else if entry.match_any(key, condition) {
                    result.push(entry);
                }
            }
        } else {
            for entry in &self.entries {
                result.push(entry);
            }
        }

        Ok(Self {
            entries: result,
            root: self.root,
        })
    }

    #[allow(unused)]
    pub fn find_else<F>(&self, cb: F) -> Result<Self, MemoError>
    where
        F: Fn(&MemoEntry) -> bool,
    {
        let mut result = vec![];

        for &entry in &self.entries {
            if cb(entry) {
                result.push(entry);
            }
        }

        Ok(Self {
            entries: result,
            root: self.root,
        })
    }

    pub fn entries(&self) -> Vec<&MemoEntry> {
        self.entries.clone()
    }

    #[allow(unused)]
    pub fn root(&self) -> &str {
        self.root
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<'a> PartialEq for MemoSearch<'a> {
    fn eq(&self, other: &Self) -> bool {
        if self.root != other.root {
            return false;
        }

        if self.entries.len() != other.entries.len() {
            return false;
        }

        for i in 0..self.entries.len() {
            if self.entries[i] != other.entries[i] {
                return false;
            }
        }

        true
    }
}

impl<'a> Add for MemoSearch<'a> {
    type Output = Result<MemoSearch<'a>, MemoError>;

    fn add(mut self, rhs: Self) -> Self::Output {
        if self.root != rhs.root {
            return Err(Report::new(MemoError::InvalidValue));
        }

        for &entry in &rhs.entries {
            if !self.entries.iter().any(|&a| a == entry) {
                self.entries.push(entry);
            }
        }

        Ok(self)
    }
}

impl<'a> Sub for MemoSearch<'a> {
    type Output = Result<MemoSearch<'a>, MemoError>;
    fn sub(mut self, rhs: Self) -> Self::Output {
        if self.root != rhs.root {
            return Err(Report::new(MemoError::InvalidValue));
        }

        self.entries
            .retain(|&entry| !rhs.entries.iter().any(|&a| a == entry));
        Ok(self)
    }
}

impl<'a> Mul for MemoSearch<'a> {
    type Output = Result<MemoSearch<'a>, MemoError>;
    fn mul(mut self, rhs: Self) -> Self::Output {
        if self.root != rhs.root {
            return Err(Report::new(MemoError::InvalidValue));
        }

        self.entries
            .retain(|&entry| rhs.entries.iter().any(|&a| a == entry));
        Ok(self)
    }
}
