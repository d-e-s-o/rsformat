// Copyright (C) 2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsStr;
use std::io;
use std::io::BufRead as _;
use std::io::BufReader;
use std::io::Error;
use std::ops::RangeInclusive;
use std::process::Child;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;

use diff_parse::File;

use serde::ser::SerializeTuple as _;
use serde::Serialize;
use serde::Serializer;
use serde_json::to_string as to_json;


/// The `rustfmt` used by default.
pub const RUSTFMT: &str = "rustfmt";


/// Serialize a `RangeInclusive`.
fn serialize_range<S>(range: &RangeInclusive<usize>, serializer: S) -> Result<S::Ok, S::Error>
where
  S: Serializer,
{
  let mut tup = serializer.serialize_tuple(2)?;
  let () = tup.serialize_element(range.start())?;
  let () = tup.serialize_element(range.end())?;
  tup.end()
}


#[derive(Debug, Serialize)]
struct FormatDescriptor<'file> {
  /// The file
  #[serde(rename = "file")]
  file: &'file str,
  #[serde(rename = "range", serialize_with = "serialize_range")]
  range: RangeInclusive<usize>,
}


/// Wait for a child process to finish and map failures to an
/// appropriate error.
fn await_child<S>(program: S, child: Child) -> io::Result<Option<ChildStdout>>
where
  S: AsRef<OsStr>,
{
  let mut child = child;

  let status = child.wait()?;
  if !status.success() {
    let error = format!("process `{}` failed", program.as_ref().to_string_lossy());

    if let Some(stderr) = child.stderr {
      let mut stderr = BufReader::new(stderr);
      let mut line = String::new();

      // Let's try to include the first line of the error output in our
      // error, to at least give the user something.
      if stderr.read_line(&mut line).is_ok() {
        let line = line.trim();
        return Err(Error::other(format!("{error}: {line}")))
      }
    }
    return Err(Error::other(error))
  }
  Ok(child.stdout)
}


/// Invoke `rustfmt` to format all the diff hunks.
pub fn format(diffs: &[(File, File)]) -> io::Result<()> {
  fn format_now(file: &str, descrs: &[FormatDescriptor]) -> io::Result<()> {
    let json = to_json(descrs).unwrap();
    let child = Command::new(RUSTFMT)
      .arg("--unstable-features")
      .arg(file)
      .arg("--file-lines")
      .arg(&json)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::piped())
      .spawn()?;
    let _ = await_child(RUSTFMT, child)?;
    Ok(())
  }

  let mut last_dst = Option::<String>::None;
  let mut descr = Vec::new();

  for (_, dst) in diffs {
    match last_dst {
      Some(prev_dst) if prev_dst != *dst.file => {
        let () = format_now(&prev_dst, &descr)?;
        last_dst = Some(dst.file.to_string());
        descr.clear();
      },
      _ => (),
    }

    let () = descr.push(FormatDescriptor {
      file: &dst.file,
      range: dst.line..=dst.line + dst.count,
    });

    if last_dst.is_none() {
      last_dst = Some(dst.file.to_string());
    }
  }

  if let Some(prev_dst) = last_dst {
    let () = format_now(&prev_dst, &descr)?;
  }
  Ok(())
}
