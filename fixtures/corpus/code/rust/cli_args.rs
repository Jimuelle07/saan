//! Tiny hand-rolled command line parser for the `photo-renamer` tool.
//! Usage: photo-renamer [--dry-run] [--prefix NAME] <directory>

use std::path::PathBuf;

#[derive(Debug, Default, PartialEq)]
pub struct Options {
    pub dry_run: bool,
    pub prefix: Option<String>,
    pub directory: PathBuf,
}

#[derive(Debug, PartialEq)]
pub enum ArgError {
    MissingValue(&'static str),
    UnknownFlag(String),
    MissingDirectory,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Options, ArgError> {
    let mut opts = Options::default();
    let mut directory = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dry-run" | "-n" => opts.dry_run = true,
            "--prefix" | "-p" => {
                opts.prefix = Some(it.next().ok_or(ArgError::MissingValue("--prefix"))?);
            }
            flag if flag.starts_with('-') => return Err(ArgError::UnknownFlag(flag.to_string())),
            path => directory = Some(PathBuf::from(path)),
        }
    }
    opts.directory = directory.ok_or(ArgError::MissingDirectory)?;
    Ok(opts)
}

#[test]
fn parses_flags_and_directory() {
    let args = ["--dry-run", "--prefix", "vacation", "./photos"].map(String::from);
    let o = parse(args).unwrap();
    assert!(o.dry_run);
    assert_eq!(o.prefix.as_deref(), Some("vacation"));
    assert_eq!(o.directory, PathBuf::from("./photos"));
}
