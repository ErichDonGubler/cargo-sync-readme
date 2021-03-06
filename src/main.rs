//! # `cargo sync-readme`
//!
//! **A plugin that generates a Markdown section in your README based on your Rust documentation.**
//!
//! ## How does this work?
//!
//! Basically, this tool provides you with a simple mechanism to synchronize your front page
//! documentation from your `lib.rs` or `main.rs` with a place in your *readme* file. In order to do
//! so, this command will parse your inner documentation (i.e. `//!`) on `lib.rs` or `main.rs` and
//! will output it in your *readme* file at specific markers.
//!
//! ## The markers
//!
//! Because you might want a specific *readme* file that adds some more information to the
//! documentation from your Rust code, this tool allows you to select a place where to put the
//! documentation. This is done with three markers:
//!
//! - `<!-- cargo-sync-readme -->`: that annotation must be placed in your *readme* file where you
//!   want the Rust documentation to be included and synchronized.
//! - `<!-- cargo-sync-readme start -->`: that annotation is automatically inserted by the command
//!   to delimit the beginning of the synchronized documentation.
//! - `<!-- cargo-sync-readme end -->`: that annotation is automatically inserted by the command
//!   to delimit the ending of the synchronized documentation.
//!
//! **You only have to use the former marker (i.e. `<!-- cargo-sync-readme -->`).** The rest of the
//! markers will be handled automatically for you by the tool.
//!
//! > Okay, but I want to change the place of the documentation now.
//!
//! When you have already put the synchronized documentation in your *readme* but want to change its
//! location, all you have to do is remove everything in between the start and end annotations
//! (annotations included) and place the `<!-- cargo-sync-readme -->` annotation wherever you want
//! your synchronized documentation to appear.
//!
//! ## How should I use this?
//!
//! First, this tool will respect what you put in your `Cargo.toml`. There is a special field called
//! `readme` that gives the name / path of the document you want to use as *readme* file.
//! `cargo sync-readme` will operate on that file.
//!
//! > Disclaimer: even though crates.io’s documentation and manifest format doesn’t explicitly state
//! > the type of this file, **`cargo sync-readme` assumes it’s Markdown.** If you want a support
//! > for another file type, please open an issue or a PR: those are warmly welcomed — and if you
//! > live in Paris, I offer you a Kwak or a Chouffe! ♥
//!
//! Once you have put the annotation in your *readme* file, just run the command without argument to
//! perform the synchronization:
//!
//! ```text
//! cargo sync-readme
//! ```
//!
//! This will effectively update your *readme* file with the main documentation from your Rust code
//! (either a `lib.rs` or `main.rs`, depending on the type of your crate).
//!
//! ## Intra-link support
//!
//! > This feature is new and lacks testing.
//!
//! This tool rewrites intra-links so they point at the corresponding place in
//! [docs.rs](https://docs.rs). The intra-links must be of the form `[⋯](crate::⋯)`.
//!
//! The regular shortcut notation (using `[foo]: crate::foo` at the end of your Markdown document
//! and using `[foo]` everywhere else) is not currently supported.
//!
//! Links to the standard library are also supported, and they must be of the form
//! `[⋯](::<crate>::⋯)`, where `<crate>` is a crate that is part of the standard library, such as
//! `std`, `core`, or `alloc`.
//!
//! Please note that there are some limitations to intra-link support. To create the links we have
//! to parse the source code to find out the class of the symbol being referenced (whether it is a
//! `struct`, `trait`, etc). That necessarily imposes some restrictions, for instance, we will not
//! expand macros so symbols defined in macros will not be linkable.
//!
//! ## Switches and options
//!
//! The command has several options and flags you can use to customize the experience (a bit like a
//! Disneyland Parc experience, but less exciting).
//!
//! - `-z` or `--show-hidden-doc`: this flag will disable a special transformation on your
//!   documentation when copying into the region you’ve selected in your *readme*. All
//!   ignored / hidden lines (the ones starting with a dash in code block in Rust doc) will simply
//!   be dropped by default. This might be wanted if you want your *readme* documentation to look
//!   like the one on docs.rs, where the hidden lines don’t show up. If you don’t, use this flag
//!   to disable this behavior.
//! - `-f` or `--prefer-doc-from`: this option allows you to override the place where to get the
//!   documentation from. This might be wanted to override the default behavior that reads from
//!   the Cargo.toml manifest, the autodetection based on files or when you have both a binary
//!   and library setup (in which case this option is mandatory).
//! - `--crlf`: this flag makes the tool’s newlines behaves according to CRLF. It will not change
//!   the already present newlines but expect your document to be formatted with CRLF. If it’s
//!   not then you will get punched in the face by a squirrel driving a motorcycle. Sorry. Also,
//!   it will generate newlines with CRLF.
//! - `-c --check`: check whether the *readme* is synchronized.
//!
//! ## Q/A and troubleshooting
//!
//! ### Are workspace crates supported?
//!
//! Not yet! If you have ideas how the tool should behave with them, please contribute with an issue or
//! a PR!

use std::{env::current_dir, fmt, fs::File, io::Write, process};
use structopt::StructOpt;

use cargo_sync_readme::{
  extract_inner_doc, read_readme, transform_readme, FindManifestError, Manifest, PreferDocFrom,
  TransformError, WithWarnings,
};

#[derive(Debug, StructOpt)]
#[structopt(author)]
enum CliOpt {
  #[structopt(
    about = "Generate a Markdown section in your README based on your Rust documentation."
  )]
  SyncReadme {
    #[structopt(
      short = "z",
      long,
      help = "Show Rust hidden documentation lines in the generated README."
    )]
    show_hidden_doc: bool,

    #[structopt(
      short = "f",
      long,
      help = "Set to either `bin` or `lib` to instruct sync-readme which file it should read documentation from."
    )]
    prefer_doc_from: Option<PreferDocFrom>,

    #[structopt(
      long,
      help = "Generate documentation with CRLF for Windows-style line endings. This will not affect the already present newlines."
    )]
    crlf: bool,

    #[structopt(short, long, help = "Check whether the README is synchronized.")]
    check: bool,
  },
}

const CANNOT_FIND_ENTRY_POINT_ERR_STR: &str = "\
Cannot find entry point (default to src/lib.rs or src/main.rs). This is likely to be due to a
special configuration in your Cargo.toml manifest file or you’re just missing the entry point
files.

If you’re in the special situation where your crate defines both a binary and a library, you should
consider using the -f option to hint sync-readme which file it should read the documentation from.";

#[derive(Debug)]
enum RuntimeError {
  FindManifestError(FindManifestError),
  CannotFindEntryPoint,
  HardError(String),
  HadWarnings,
  TransformError(TransformError),
  NotSynchronized,
}

impl fmt::Display for RuntimeError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      RuntimeError::FindManifestError(ref e) => write!(f, "{}", e),
      RuntimeError::CannotFindEntryPoint => write!(f, "{}", CANNOT_FIND_ENTRY_POINT_ERR_STR),
      RuntimeError::HardError(ref e) => write!(f, "{}", e),
      RuntimeError::HadWarnings => f.write_str("there were warnings"),
      RuntimeError::TransformError(ref e) => write!(f, "{}", e),
      RuntimeError::NotSynchronized => f.write_str("the README is not synchronized!"),
    }
  }
}

impl From<FindManifestError> for RuntimeError {
  fn from(err: FindManifestError) -> Self {
    Self::FindManifestError(err)
  }
}

impl From<TransformError> for RuntimeError {
  fn from(err: TransformError) -> Self {
    Self::TransformError(err)
  }
}

impl RuntimeError {
  fn hard_error(e: impl Into<String>) -> Self {
    RuntimeError::HardError(e.into())
  }
}

fn main() {
  let cli_opt = CliOpt::from_args();

  if let Ok(pwd) = current_dir() {
    let run = Manifest::find_manifest(pwd)
      .map_err(RuntimeError::from)
      .and_then(|manifest| run_with_manifest(manifest, cli_opt));
    if let Err(e) = run {
      eprintln!("{}", e);
      process::exit(1);
    }
  } else {
    eprintln!("It seems like you’re running this command from nowhere good…");
    process::exit(1);
  }
}

fn run_with_manifest(manifest: Manifest, cli_opt: CliOpt) -> Result<(), RuntimeError> {
  let CliOpt::SyncReadme {
    prefer_doc_from,
    show_hidden_doc,
    crlf,
    check,
    ..
  } = cli_opt;

  let crate_name = manifest
    .crate_name()
    .ok_or_else(|| RuntimeError::hard_error("Failed to get the name of the crate"))?;
  let entry_point = manifest.entry_point(prefer_doc_from);

  if let Some(entry_point) = entry_point {
    let doc = extract_inner_doc(&entry_point, show_hidden_doc, crlf)?;
    let readme_path = manifest.readme();
    let (old_readme, new_readme_with_warnings) = read_readme(&readme_path).and_then(|readme| {
      transform_readme(&readme, doc, crate_name, entry_point, crlf)
        .map(|new_readme_with_warnings| (readme, new_readme_with_warnings))
    })?;
    let WithWarnings {
      value: new_readme,
      warnings,
    } = new_readme_with_warnings;

    for w in &warnings {
      eprintln!("{}", w);
    }

    if check {
      report_synchronized(&old_readme, &new_readme)
    } else {
      let mut file = File::create(readme_path).unwrap();
      let _ = file.write_all(new_readme.as_bytes());

      if warnings.is_empty() {
        Ok(())
      } else {
        Err(RuntimeError::HadWarnings)
      }
    }
  } else {
    Err(RuntimeError::CannotFindEntryPoint)
  }
}

fn report_synchronized(old: &str, new: &str) -> Result<(), RuntimeError> {
  if old != new {
    Err(RuntimeError::NotSynchronized)
  } else {
    Ok(())
  }
}
