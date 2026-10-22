//! Reads only what was added to a log file since last time, like `tail -f`.
//!
//! The agents append to their logs after every turn, and the logs get big (tens of MB). Re-reading every
//! file from the start on each change would be slow, so a `Tail` remembers how far it got.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// How far we've read into one file.
#[derive(Debug, Default)]
pub struct Tail {
    /// Bytes read so far.
    offset: u64,
    /// The end of the file when it doesn't end in a newline yet: a line the agent is still writing.
    /// It's kept until the rest of the line arrives.
    partial: Vec<u8>,
    /// Which file this was (device and inode), to notice when it's replaced by a new file.
    id: Option<(u64, u64)>,
}

/// New complete lines since the last read.
#[derive(Debug, PartialEq, Eq)]
pub struct NewLines {
    pub lines: Vec<String>,
    /// True when the file got shorter or was replaced, so these lines start again from the top. Whatever
    /// was worked out from the old lines may be out of date.
    pub restarted: bool,
}

impl Tail {
    pub fn read(&mut self, path: &Path) -> io::Result<NewLines> {
        let mut file = File::open(path)?;
        let meta = file.metadata()?;

        let id = file_id(&meta);
        let restarted = meta.len() < self.offset || (self.id.is_some() && id != self.id);
        if restarted {
            *self = Tail::default();
        }
        self.id = id;

        if meta.len() == self.offset {
            return Ok(NewLines {
                lines: Vec::new(),
                restarted,
            });
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = std::mem::take(&mut self.partial);
        let read = file.read_to_end(&mut bytes)?;
        self.offset += read as u64;

        // Everything up to the last newline is complete lines; whatever comes after it waits for next time.
        let complete = match bytes.iter().rposition(|&b| b == b'\n') {
            Some(last) => {
                self.partial = bytes.split_off(last + 1);
                bytes
            }
            None => {
                self.partial = bytes;
                Vec::new()
            }
        };
        let lines = complete
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect();
        Ok(NewLines { lines, restarted })
    }
}

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_id(_meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_file(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("statuspal-test-tail-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("log.jsonl")
    }

    fn append(path: &Path, text: &str) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    fn lines(new: NewLines) -> Vec<String> {
        new.lines
    }

    #[test]
    fn reads_only_what_was_added() {
        let path = temp_file("added");
        let mut tail = Tail::default();
        append(&path, "one\ntwo\n");
        assert_eq!(lines(tail.read(&path).unwrap()), ["one", "two"]);
        assert!(lines(tail.read(&path).unwrap()).is_empty());
        append(&path, "three\n");
        assert_eq!(lines(tail.read(&path).unwrap()), ["three"]);
    }

    #[test]
    fn waits_for_the_end_of_a_half_written_line() {
        let path = temp_file("partial");
        let mut tail = Tail::default();
        append(&path, "one\ntw");
        assert_eq!(lines(tail.read(&path).unwrap()), ["one"]);
        append(&path, "o\n");
        assert_eq!(lines(tail.read(&path).unwrap()), ["two"]);
    }

    #[test]
    fn starts_over_when_the_file_shrinks() {
        let path = temp_file("shrink");
        let mut tail = Tail::default();
        append(&path, "one\ntwo\n");
        tail.read(&path).unwrap();

        fs::write(&path, "new\n").unwrap();
        let new = tail.read(&path).unwrap();
        assert!(new.restarted);
        assert_eq!(new.lines, ["new"]);
    }

    #[test]
    fn starts_over_when_the_file_is_replaced() {
        let path = temp_file("replace");
        let mut tail = Tail::default();
        append(&path, "one\n");
        tail.read(&path).unwrap();

        // A new file renamed over the old one, longer than what we'd read.
        let other = path.with_extension("tmp");
        fs::write(&other, "fresh\nlines\n").unwrap();
        fs::rename(&other, &path).unwrap();
        let new = tail.read(&path).unwrap();
        assert!(new.restarted);
        assert_eq!(new.lines, ["fresh", "lines"]);
    }
}
