//! One `Read + Seek` view over a numerically split archive's volumes.
//!
//! # Why this exists, and why it is this small
//!
//! A 7z multi-volume set is **a plain byte split of one archive**. Concatenating
//! `big.7z.001`, `big.7z.002` and the rest reproduces `big.7z` byte for byte; no
//! part carries a header, a footer, or a volume number. Three things establish
//! that, and they agree: the 7z format specification has no concept of a volume
//! at all; 7-Zip handles `.001` sets through a separate pseudo-format registered
//! with `_NO_SIG`, i.e. the split container has no magic bytes of its own; and
//! the stream 7-Zip hands to its real parser is a concatenating adapter with no
//! header skipping anywhere in it. Verified here as well, on this machine: an
//! archive written with `7zz a -v64k` and then `cat`-ed back together `cmp`s
//! identical to the same content written unsplit.
//!
//! So supporting a split set needs no format knowledge whatsoever. It needs a
//! reader that makes N files look like their concatenation, which is this file.
//!
//! **RAR is the instructive contrast.** RAR volumes are *not* a byte split —
//! each one is a self-describing archive carrying `MHD_VOLUME` and a volume
//! number, which is why RAR reassembly lives inside the UnRAR SDK and why this
//! crate had to teach its listing walk about continuation headers. Nothing of
//! that kind applies here, and a reader who assumes the two are the same
//! problem will over-build this by an order of magnitude.
//!
//! # What it does not do
//!
//! It does not decide *which* files form the set — `crate::format::multipart`
//! already does that, and it is the one place that should. This type takes an
//! ordered list of paths and presents their concatenation.
//!
//! It also does not detect a gap. A missing middle volume in a byte-split set
//! cannot be seen from the bytes: the parts after the gap simply appear at the
//! wrong offsets, so the archive reads as corrupt rather than as incomplete.
//! That is a real difference from RAR, where a missing volume is named. The
//! completeness answer therefore has to come from the volume-set report before
//! a chain is built, not from anything observed while reading it.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

/// One member of the chain, with its position in the concatenation.
#[derive(Debug)]
struct Part {
    file: File,
    /// Offset of this part's byte 0 within the concatenation.
    start: u64,
    len: u64,
}

/// The volumes of a split archive, presented as one contiguous source.
///
/// A single-file archive is a chain of one, and behaves exactly as the `File`
/// would. That is deliberate: it lets the 7z backend build every reader the
/// same way instead of carrying two source types through its signatures.
#[derive(Debug)]
pub(crate) struct VolumeChain {
    parts: Vec<Part>,
    total_len: u64,
    /// Logical position within the concatenation. May exceed `total_len`,
    /// which reads as EOF — the same thing a `File` does past its end.
    pos: u64,
    /// Index of the part whose own cursor is known to agree with `pos`.
    ///
    /// Every part is opened once and kept open, so there is no reopen-by-name
    /// after construction and no window for a replacement to slip through.
    /// Only this adapter drives those handles, so one lazy seek per part
    /// transition is enough; `None` means "seek before the next read".
    synced: Option<usize>,
}

impl VolumeChain {
    /// Open `paths` in the given order and present their concatenation.
    ///
    /// Every file is opened here rather than on demand. It costs one
    /// descriptor per volume, and it buys the property the rest of this crate
    /// insists on: the bytes read are the bytes of the files that were
    /// measured, with no by-name reopen in between for something else to take
    /// the name.
    pub(crate) fn open(paths: &[PathBuf]) -> io::Result<Self> {
        if paths.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a volume chain needs at least one volume",
            ));
        }

        let mut parts = Vec::with_capacity(paths.len());
        let mut start = 0u64;
        for path in paths {
            let file = File::open(path)?;
            let len = file.metadata()?.len();
            parts.push(Part { file, start, len });
            start = start.checked_add(len).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "volume set is larger than u64 can address",
                )
            })?;
        }

        Ok(VolumeChain {
            parts,
            total_len: start,
            pos: 0,
            synced: None,
        })
    }

    /// A chain of one already-open file — the ordinary, unsplit case.
    pub(crate) fn single(file: File) -> io::Result<Self> {
        let len = file.metadata()?.len();
        Ok(VolumeChain {
            parts: vec![Part {
                file,
                start: 0,
                len,
            }],
            total_len: len,
            pos: 0,
            synced: None,
        })
    }

    /// Total length of the concatenation.
    pub(crate) fn total_len(&self) -> u64 {
        self.total_len
    }

    /// How many volumes back this chain. One means an ordinary archive.
    #[cfg(test)]
    pub(crate) fn volume_count(&self) -> usize {
        self.parts.len()
    }

    /// Index of the part containing `pos`, or `None` at or past the end.
    ///
    /// Zero-length parts are skipped rather than matched: an empty volume
    /// occupies no byte range, so no position is ever "inside" it.
    fn part_at(&self, pos: u64) -> Option<usize> {
        if pos >= self.total_len {
            return None;
        }
        // Linear from the current part is the common case (sequential reads);
        // sets are small enough that the worst case does not justify a search.
        self.parts
            .iter()
            .position(|p| p.len > 0 && pos >= p.start && pos < p.start + p.len)
    }
}

impl Read for VolumeChain {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let Some(idx) = self.part_at(self.pos) else {
            return Ok(0); // at or past the end of the concatenation
        };

        let local = self.pos - self.parts[idx].start;
        let remaining_in_part = self.parts[idx].len - local;

        if self.synced != Some(idx) {
            self.parts[idx].file.seek(SeekFrom::Start(local))?;
            self.synced = Some(idx);
        }

        // Never read past this part's end: the next bytes live in a different
        // file, and a short read is a perfectly ordinary `Read` result. The
        // caller's next call picks up in the following part.
        let want = buf
            .len()
            .min(usize::try_from(remaining_in_part).unwrap_or(usize::MAX));
        let read = self.parts[idx].file.read(&mut buf[..want])?;
        if read == 0 && remaining_in_part > 0 {
            // The part is shorter than its measured length — it was truncated
            // under us. Report it rather than silently splicing the next
            // volume's bytes into the gap, which would look like corruption
            // arriving from nowhere.
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "volume {} of {} ended {remaining_in_part} bytes before its measured length",
                    idx + 1,
                    self.parts.len()
                ),
            ));
        }
        self.pos += read as u64;
        Ok(read)
    }
}

impl Seek for VolumeChain {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(n) => i128::from(self.total_len as i64) + i128::from(n),
            SeekFrom::Current(n) => self.pos as i128 + i128::from(n),
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot seek to a negative position in a volume chain",
            ));
        }
        let target = u64::try_from(target).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek target does not fit in u64",
            )
        })?;

        if target != self.pos {
            // The inner handle is no longer where `pos` says. Re-seek lazily,
            // on the next read, so a seek-heavy caller that never reads pays
            // nothing.
            self.synced = None;
            self.pos = target;
        }
        Ok(self.pos)
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        Ok(self.pos)
    }
}

#[cfg(test)]
#[path = "volume_chain/tests.rs"]
mod tests;
