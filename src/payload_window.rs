//! A `Read + Seek` view of a byte range inside a larger file.
//!
//! Self-extracting archives put a real archive at some offset inside an
//! executable. The crate's original answer was to copy those bytes out to
//! a tempfile and open that (AD 0040, and the 16 GiB ceiling that copy
//! forced). This adapter is the other answer: hand the backend a view that
//! makes byte `offset` look like byte 0, and let it read the payload where
//! it already lies.
//!
//! # Why a new type
//!
//! Nothing in the crate does this. `HardCapReader` (`crate::streaming`)
//! is `Read`-only and counts bytes to enforce a bound — it polices totals,
//! it does not translate positions, and conflating the two would give one
//! type two jobs. The libarchive stream readers are `Read`-only pulls over
//! an FFI handle. What the offset-aware opens need is a `Seek`
//! implementation over a sub-range, and no type in `src/` had one.
//!
//! # The two things that make it correct
//!
//! **`len` is frozen at construction.** Bytes appended to the underlying
//! file after the window is built are invisible. That is deliberate: it is
//! the in-place analogue of the `Read::take` bound the staging copy
//! enforces (R0075-0002), and without it a concurrently-growing SFX could
//! feed a backend bytes past the length the safety gate admitted.
//!
//! **`End`-relative seeks resolve against the window, not the file.**
//! This is not a nicety. `sevenz_rust2::Archive::read` opens by seeking
//! `End(0)` to learn the archive's length before it reads anything, so a
//! window that answered with the *file's* end would hand it a length that
//! includes the stub and every byte after the payload.

use std::io::{Read, Seek, SeekFrom};

/// A `Read + Seek` view of `[start, start + len)` of an inner source.
///
/// Positions reported and accepted by this type are window-relative:
/// window byte 0 is inner byte `start`. Seeking past the end of the window
/// is allowed and reads there return `Ok(0)`, matching how `File` behaves
/// past EOF; seeking to a negative position is `InvalidInput`, also
/// matching `File`.
#[derive(Debug)]
pub(crate) struct PayloadWindow<R: Read + Seek> {
    inner: R,
    /// Absolute offset of window byte 0 within `inner`.
    start: u64,
    /// Window length, frozen at construction. See the module docs.
    len: u64,
    /// Logical position within the window. May exceed `len`.
    pos: u64,
    /// Whether `inner`'s cursor is known to sit at `start + pos`.
    ///
    /// Only this adapter drives `inner` after construction, so the two can
    /// be kept in step with one lazy re-seek instead of a seek per read.
    /// `false` means "re-seek before the next read"; construction starts
    /// `false` so the inner source is positioned lazily rather than in
    /// `new`, which keeps `new` infallible.
    inner_synced: bool,
}

impl<R: Read + Seek> PayloadWindow<R> {
    /// A window over `[start, start + len)` of `inner`.
    ///
    /// `inner` is positioned lazily, on the first read, so this cannot
    /// fail. Neither `start` nor `len` is validated against the real size
    /// of `inner`: a window longer than what is there simply reads short,
    /// exactly as a `File` does.
    pub(crate) fn new(inner: R, start: u64, len: u64) -> Self {
        PayloadWindow {
            inner,
            start,
            len,
            pos: 0,
            inner_synced: false,
        }
    }

    /// The window's length — what an `End`-relative seek resolves against.
    #[cfg(test)]
    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// A window from `start` to the end of a source whose total length is
    /// already known.
    ///
    /// The sized variant of [`Self::from_file`], for sources that cannot be
    /// `fstat`-ed because they are not one file — a volume chain, whose length
    /// is the sum of its parts. The same "start past the end" refusal applies,
    /// and for the same reason: a caller asking for a payload that cannot exist
    /// is a bug worth surfacing rather than an empty window worth serving.
    pub(crate) fn from_sized(inner: R, start: u64, total_len: u64) -> std::io::Result<Self> {
        let len = total_len.checked_sub(start).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "payload offset {start} is past the end of the {total_len}-byte source it \
                     is supposed to index into"
                ),
            )
        })?;
        Ok(Self::new(inner, start, len))
    }
}

impl PayloadWindow<std::fs::File> {
    /// A window from `start` to the end of the file, sized by `fstat` on
    /// the already-open handle.
    ///
    /// The length comes from the descriptor rather than from a by-name
    /// `stat` on purpose: the descriptor names the exact file that will be
    /// read, so there is no window between measuring and reading for a
    /// replacement to slip through. That is the same reasoning the
    /// read-handle identity binding uses (OI-0001-002), and the reason ZIP
    /// is the one backend whose binding has no stat-to-open gap at all.
    ///
    /// Errors with `InvalidInput` when `start` is past the end of the
    /// file — a caller asking for a payload that cannot exist is a bug
    /// worth surfacing, not an empty window worth serving.
    pub(crate) fn from_file(file: std::fs::File, start: u64) -> std::io::Result<Self> {
        let file_len = file.metadata()?.len();
        let len = file_len.checked_sub(start).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "payload offset {start} is past the end of the {file_len}-byte file it \
                     is supposed to index into"
                ),
            )
        })?;
        Ok(Self::new(file, start, len))
    }
}

impl<R: Read + Seek> Read for PayloadWindow<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // At or past the frozen end: EOF, whatever the file has grown to.
        let Some(remaining) = self.len.checked_sub(self.pos) else {
            return Ok(0);
        };
        if remaining == 0 || buf.is_empty() {
            return Ok(0);
        }

        if !self.inner_synced {
            let absolute = self.start.checked_add(self.pos).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "window position overflows the inner source's address space",
                )
            })?;
            self.inner.seek(SeekFrom::Start(absolute))?;
            self.inner_synced = true;
        }

        let want = buf
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let read = match self.inner.read(&mut buf[..want]) {
            Ok(read) => read,
            Err(err) => {
                // The inner cursor is unknown after a failed read; force a
                // re-seek rather than trusting it on the next call.
                self.inner_synced = false;
                return Err(err);
            }
        };
        self.pos += read as u64;
        Ok(read)
    }
}

impl<R: Read + Seek> Seek for PayloadWindow<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let invalid = |what: &str| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("seek to {what} position in a payload window"),
            )
        };

        // `End` resolves against the WINDOW's end, not the file's. See the
        // module docs: 7z opens by asking for it.
        let resolved = match pos {
            SeekFrom::Start(offset) => return self.set_pos(offset),
            SeekFrom::Current(delta) => checked_offset(self.pos, delta),
            SeekFrom::End(delta) => checked_offset(self.len, delta),
        };
        match resolved {
            Some(next) => self.set_pos(next),
            None => Err(invalid("a negative or overflowing")),
        }
    }
}

impl<R: Read + Seek> PayloadWindow<R> {
    fn set_pos(&mut self, next: u64) -> std::io::Result<u64> {
        if next != self.pos {
            self.inner_synced = false;
        }
        self.pos = next;
        Ok(next)
    }
}

/// `base + delta` where `delta` may be negative. `None` on underflow past
/// zero or overflow past `u64::MAX`, both of which `File` reports as
/// `InvalidInput`.
fn checked_offset(base: u64, delta: i64) -> Option<u64> {
    if delta >= 0 {
        base.checked_add(delta as u64)
    } else {
        // `-delta` as u64 is safe for i64::MIN via wrapping through u64.
        base.checked_sub((delta as i128).unsigned_abs() as u64)
    }
}

#[cfg(test)]
mod tests;
