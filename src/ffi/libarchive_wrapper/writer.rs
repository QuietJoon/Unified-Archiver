//! Write-side libarchive operations: archive creation and entry writing.

use super::*;

/// Data source for `write_entry`: either a fully buffered slice or a streamed reader.
enum EntryDataSource<'a> {
    Bytes(&'a [u8]),
    Stream {
        reader: &'a mut dyn std::io::Read,
        expected_size: u64,
    },
}

/// Classify a failed `archive_write_add_filter_*` registration as
/// [`ArchiveError::CodecUnavailable`].
///
/// A libarchive built without a codec's *write* filter reports the
/// registration as `ARCHIVE_WARN`/`ARCHIVE_FATAL` here, which used to
/// surface as a generic `Format` error with libarchive's own sentence
/// and nothing actionable. The typed variant names the codec and
/// carries the per-platform install instructions the error module
/// already maintains, so the caller can tell "your libarchive lacks
/// zstd" apart from "this archive is malformed".
///
/// libarchive's own text is appended to the instructions rather than
/// dropped: it is the only place the native reason (external-program
/// fallback, unknown option, …) is recorded.
fn codec_unavailable_with_detail(
    codec: &str,
    format: crate::ArchiveFormat,
    detail: &str,
) -> ArchiveError {
    let instructions = ArchiveError::codec_install_instructions(codec);
    let install_instructions = if detail.trim().is_empty() {
        instructions
    } else {
        format!("{instructions} (libarchive reported: {detail})")
    };
    ArchiveError::CodecUnavailable {
        codec: codec.to_string(),
        format,
        install_instructions,
    }
}

/// RAII guard for a libarchive entry pointer; frees on drop so error paths
/// can use `?` without leaking.
struct EntryGuard {
    entry: *mut LibarchiveEntry,
}

impl EntryGuard {
    /// Allocate a new libarchive entry. Caller is responsible for the unsafety
    /// of subsequent libarchive calls using the pointer.
    fn new() -> Result<Self> {
        let entry = unsafe { archive_entry_new() };
        if entry.is_null() {
            Err(ArchiveError::format(None, "Failed to create entry"))
        } else {
            Ok(Self { entry })
        }
    }
}

impl Drop for EntryGuard {
    fn drop(&mut self) {
        if !self.entry.is_null() {
            unsafe { archive_entry_free(self.entry) };
        }
    }
}

impl LibarchiveArchive {
    /// Create a new archive for writing
    pub fn create(
        path: impl AsRef<Path>,
        format: crate::ArchiveFormat,
        options: &mut crate::options::CompressionOptions,
    ) -> Result<Self> {
        // Encrypted archive creation is not supported in any format (MADR-0027).
        // Reject the password before allocating any libarchive state so the
        // policy rejection doesn't leave an opened native writer behind, and
        // the ordering matches the ZIP backend's create-time check
        // (R0071-0011 — AD 0059 closure had drifted).
        if options.password.is_some() {
            return Err(ArchiveError::operation_blocked(
                crate::error::ops::CREATE,
                format!(
                    "Encrypted archive creation is not supported by this library (format={:?}). Use open_encrypted() to read existing encrypted archives.",
                    format
                ),
            ));
        }

        let path_str = path.as_ref().to_string_lossy().to_string();
        // Reject NULs early so the path can be safely surfaced in error
        // messages; libarchive itself never sees a `*const c_char` from
        // this path now that `archive_write_open_fd` carries the fd.
        if path_str.as_bytes().contains(&0) {
            return Err(ArchiveError::invalid_path(&path_str, "Contains null byte"));
        }

        unsafe {
            let archive = archive_write_new();
            if archive.is_null() {
                return Err(ArchiveError::format(
                    Some(format),
                    "Failed to create write archive handle",
                ));
            }

            // Which compression filter the arm below registered, if
            // any. `Some(codec)` means the only remaining failure the
            // shared check can see came from the *filter* registration
            // (each arm returns early on its own format-setup failure),
            // so it is classified as `CodecUnavailable` naming that
            // codec; `None` keeps the generic `Format` classification.
            let mut filter_codec: Option<&'static str> = None;

            // Set format. For tar-with-filter formats both the format
            // *and* the filter setup return codes must be checked
            // (R0070-0042). The previous code discarded
            // `archive_write_set_format_pax_restricted`'s result and
            // only inspected the filter, so a format-setup failure
            // surfaced as an opaque parse error from the first
            // `write_header` call.
            let format_result = match format {
                // **Deliberately not the ZIP writer** (AD-0013 amendment
                // 2026-09-03, ti-9909d449). `Archive::create` routes
                // `ArchiveFormat::Zip` to `ZipWriter` (the `zip` crate) and
                // sends only the remaining formats here, so the facade never
                // reaches this arm. It is kept, not deleted, because
                // `LibarchiveArchive::create` is reachable directly through
                // the public `ffi` module: an external caller *can* hand it
                // `Zip`, and a `_ => unsupported` fallthrough would answer
                // that with a libarchive parse failure from the first
                // `write_header` instead of a configured writer. Wiring the
                // facade to this arm would silently drop everything the ZIP
                // backend owns (MADR-0027 encryption policy, the namespace
                // tracker, CRC handling) — if ZIP creation ever moves to
                // libarchive, that is a decision record, not a one-line
                // redirect in `creation.rs`.
                crate::ArchiveFormat::Zip => archive_write_set_format_zip(archive),
                crate::ArchiveFormat::SevenZip => archive_write_set_format_7zip(archive),
                crate::ArchiveFormat::Tar => archive_write_set_format_pax_restricted(archive),
                crate::ArchiveFormat::TarGzip => {
                    let r = archive_write_set_format_pax_restricted(archive);
                    if r != ARCHIVE_OK {
                        let error_msg = get_archive_error(archive);
                        archive_write_free(archive);
                        return Err(ArchiveError::format(Some(format), error_msg));
                    }
                    filter_codec = Some("gzip");
                    archive_write_add_filter_gzip(archive)
                }
                crate::ArchiveFormat::TarBzip2 => {
                    let r = archive_write_set_format_pax_restricted(archive);
                    if r != ARCHIVE_OK {
                        let error_msg = get_archive_error(archive);
                        archive_write_free(archive);
                        return Err(ArchiveError::format(Some(format), error_msg));
                    }
                    filter_codec = Some("bzip2");
                    archive_write_add_filter_bzip2(archive)
                }
                crate::ArchiveFormat::TarXz => {
                    let r = archive_write_set_format_pax_restricted(archive);
                    if r != ARCHIVE_OK {
                        let error_msg = get_archive_error(archive);
                        archive_write_free(archive);
                        return Err(ArchiveError::format(Some(format), error_msg));
                    }
                    filter_codec = Some("xz");
                    archive_write_add_filter_xz(archive)
                }
                crate::ArchiveFormat::TarZst => {
                    let r = archive_write_set_format_pax_restricted(archive);
                    if r != ARCHIVE_OK {
                        let error_msg = get_archive_error(archive);
                        archive_write_free(archive);
                        return Err(ArchiveError::format(Some(format), error_msg));
                    }
                    // A libarchive built without libzstd registers the
                    // filter as an external-program fallback and returns
                    // ARCHIVE_WARN here; the shared `format_result` check
                    // below converts that into a hard `CodecUnavailable`
                    // error instead of silently shelling out (R0070-0041
                    // loud failure posture).
                    filter_codec = Some("zstd");
                    archive_write_add_filter_zstd(archive)
                }
                crate::ArchiveFormat::TarLz4 => {
                    let r = archive_write_set_format_pax_restricted(archive);
                    if r != ARCHIVE_OK {
                        let error_msg = get_archive_error(archive);
                        archive_write_free(archive);
                        return Err(ArchiveError::format(Some(format), error_msg));
                    }
                    // Same external-program-fallback caveat as zstd above.
                    filter_codec = Some("lz4");
                    archive_write_add_filter_lz4(archive)
                }
                crate::ArchiveFormat::TarLzma => {
                    let r = archive_write_set_format_pax_restricted(archive);
                    if r != ARCHIVE_OK {
                        let error_msg = get_archive_error(archive);
                        archive_write_free(archive);
                        return Err(ArchiveError::format(Some(format), error_msg));
                    }
                    filter_codec = Some("lzma");
                    archive_write_add_filter_lzma(archive)
                }
                _ => {
                    archive_write_free(archive);
                    // Keep the public operation label
                    // stable (`ops::CREATE`) and put the unsupported
                    // format in the reason text. The previous
                    // `format!("create {format:?}")` produced a
                    // dynamic operation string that downstream callers
                    // could not match on consistently.
                    return Err(ArchiveError::operation_blocked(
                        crate::error::ops::CREATE,
                        format!("Format {format:?} is not supported for creation"),
                    ));
                }
            };

            if format_result != ARCHIVE_OK {
                let error_msg = get_archive_error(archive);
                archive_write_free(archive);
                return Err(match filter_codec {
                    Some(codec) => codec_unavailable_with_detail(codec, format, &error_msg),
                    None => ArchiveError::format(Some(format), error_msg),
                });
            }

            // Set compression level for all supported formats
            {
                use crate::options::CompressionLevel;
                let compression_value = match options.level {
                    CompressionLevel::Store => "0",
                    CompressionLevel::Fastest => "1",
                    CompressionLevel::Fast => "3",
                    CompressionLevel::Normal => "6",
                    CompressionLevel::Maximum => "8",
                    CompressionLevel::Ultra => "9",
                };

                // Surface compression-level setup failures rather than
                // letting libarchive silently fall back to the codec's
                // default (R0070-0041). `ARCHIVE_WARN` is treated as
                // "option ignored" — the caller's request was not
                // honored, so we propagate `Format` instead of swallowing.
                let set_level_result = match format {
                    crate::ArchiveFormat::TarGzip
                    | crate::ArchiveFormat::TarBzip2
                    | crate::ArchiveFormat::TarXz
                    | crate::ArchiveFormat::TarZst
                    | crate::ArchiveFormat::TarLz4
                    | crate::ArchiveFormat::TarLzma => {
                        // zstd has no "no compression" level: its filter
                        // passes the value straight to libzstd, where 0
                        // selects the library default (3). Map `Store` to
                        // the weakest real level instead so the request
                        // stays directionally honest.
                        let value = if format == crate::ArchiveFormat::TarZst
                            && options.level == CompressionLevel::Store
                        {
                            "1"
                        } else {
                            compression_value
                        };
                        if let (Ok(opt), Ok(val)) =
                            (CString::new("compression-level"), CString::new(value))
                        {
                            Some(archive_write_set_filter_option(
                                archive,
                                std::ptr::null(),
                                opt.as_ptr(),
                                val.as_ptr(),
                            ))
                        } else {
                            None
                        }
                    }
                    // 7z is live here; `Zip` is the same deliberately
                    // unwired path as the format arm above (AD-0013
                    // amendment 2026-09-03, ti-9909d449) — the facade's
                    // ZIP levels are mapped by `ZipWriter`, and AD-0013's
                    // "Libarchive ZIP" bullet describes this option call,
                    // not the one users actually get. It shares 7z's arm
                    // because the option is set the same way
                    // (`archive_write_set_format_option`, a *format*
                    // option, unlike the filter option the TAR.* arm
                    // above uses), so keeping it costs nothing and keeps
                    // a directly-constructed libarchive ZIP writer
                    // honouring the caller's level.
                    crate::ArchiveFormat::Zip | crate::ArchiveFormat::SevenZip => {
                        if let (Ok(opt), Ok(val)) = (
                            CString::new("compression-level"),
                            CString::new(compression_value),
                        ) {
                            Some(archive_write_set_format_option(
                                archive,
                                std::ptr::null(),
                                opt.as_ptr(),
                                val.as_ptr(),
                            ))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if matches!(set_level_result, Some(r) if r != ARCHIVE_OK) {
                    let error_msg = get_archive_error(archive);
                    archive_write_free(archive);
                    return Err(ArchiveError::format(
                        Some(format),
                        format!("compression-level option rejected: {}", error_msg),
                    ));
                }
            }

            // Open output file exclusively before handing the descriptor to
            // libarchive. `OpenOptions::create_new(true)` is `O_CREAT|O_EXCL`,
            // so a competing process cannot swap or pre-create the inode
            // between the facade's `path.exists()` precheck and this open
            // (R0072-0004). The descriptor is then attached via
            // `archive_write_open_fd`, mirroring the ZIP backend's exclusive
            // path. The owned `File` is held in `write_output` so the kernel
            // descriptor stays valid for libarchive's lifetime — dropping
            // before `archive_write_close` would close it under libarchive's
            // feet.
            let output_file = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.as_ref())
            {
                Ok(f) => f,
                Err(e) => {
                    archive_write_free(archive);
                    return Err(ArchiveError::io("write_open", path_str.clone(), e));
                }
            };

            #[cfg(unix)]
            let raw_fd: c_int = {
                use std::os::unix::io::AsRawFd;
                // Unix: `as_raw_fd` returns the file descriptor by
                // borrow. `archive_write_open_fd` does not close the
                // fd (libarchive's documented contract); `output_file`
                // remains the sole owner and closes the fd on Drop.
                output_file.as_raw_fd()
            };
            #[cfg(windows)]
            let raw_fd: c_int = {
                use std::os::windows::io::IntoRawHandle;
                // R0075-0028: `_open_osfhandle` *takes ownership* of
                // the OS HANDLE it wraps. Closing the resulting CRT
                // fd via `_close` closes the underlying HANDLE; if
                // we kept `output_file` alive its Drop would also try
                // to close the same HANDLE, doubling the close. Use
                // `into_raw_handle` to consume the `File` without
                // closing — ownership now flows
                // `File` → HANDLE → CRT fd, and the CRT fd is the
                // sole long-term owner. We deliberately do NOT keep
                // `write_output` populated on Windows for this reason.
                //
                // The CRT fd is leaked on success-with-libarchive-not-
                // closing: `archive_write_open_fd` does not close the
                // fd. Closing it ourselves at `close_write` time
                // requires a Windows-only field; that wiring lands as
                // part of OI-0065-001 (Windows libarchive build
                // enablement) when the path is actually exercised.
                // Until then this Windows arm compiles correctly and
                // documents the ownership story; it is gated behind
                // `cfg(windows)` and is not reached by the default
                // build matrix.
                let handle = output_file.into_raw_handle();
                let crt_fd = libc::_open_osfhandle(handle as libc::intptr_t, 0);
                if crt_fd < 0 {
                    // We took ownership of the HANDLE via
                    // `into_raw_handle` and `_open_osfhandle` did not
                    // bind it. Re-wrap the HANDLE in a `File` so its
                    // Drop closes it cleanly.
                    use std::os::windows::io::FromRawHandle;
                    let _reclaim = std::fs::File::from_raw_handle(handle);
                    archive_write_free(archive);
                    return Err(ArchiveError::io(
                        "write_open",
                        path_str.clone(),
                        std::io::Error::other("_open_osfhandle failed"),
                    ));
                }
                crt_fd
            };

            let open_result = archive_write_open_fd(archive, raw_fd);
            if open_result != ARCHIVE_OK {
                let error_msg = get_archive_error(archive);
                archive_write_free(archive);
                // On Unix: `output_file` drops here, closing the
                // exclusive fd. On Windows: the CRT fd has ownership
                // of the HANDLE; calling `_close(crt_fd)` cleans up
                // both fd and HANDLE.
                #[cfg(windows)]
                {
                    libc::_close(raw_fd);
                }
                return Err(ArchiveError::io(
                    "write_open",
                    path_str.clone(),
                    std::io::Error::other(error_msg),
                ));
            }

            Ok(Self {
                path: PathBuf::from(path_str),
                write_handle: Some(archive),
                #[cfg(unix)]
                write_output: Some(output_file),
                #[cfg(windows)]
                write_output: None, // see R0075-0028 comment above
                progress: options.progress.take(),
                bytes_written: 0,
                entries_written: 0,
                stream_buffer: Vec::new(),
                write_poisoned: false,
                finish_failure: None,
                cached_listing: OnceCell::new(),
                // OI-0001-002: the identity binding exists to prove a
                // *read* handle is still looking at the file its cached
                // listing describes. A write handle has no cached
                // listing and never re-opens by path, so there is
                // nothing to bind and `None` is the honest value — not
                // a capture that failed.
                identity: None,
                // Write-mode handles read no headers, so nothing can
                // ever land here.
                backend_warnings: std::cell::RefCell::new(Vec::new()),
            })
        }
    }

    fn notify_progress(&mut self, additional: u64) -> Result<()> {
        crate::ffi::common::notify_creation_progress(
            &mut self.progress,
            &mut self.bytes_written,
            additional,
        )
    }

    /// Add a file to the archive from byte data
    pub fn add_file_from_data(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        let size = data.len() as u64;
        self.write_entry(
            archive_path,
            size,
            0o644,
            None,
            None,
            None,
            EntryDataSource::Bytes(data),
        )
    }

    /// Add a file from byte data, preserving metadata from an existing `ArchiveEntry`.
    ///
    /// Used by `commit_changes()` to round-trip entries without losing timestamps
    /// and permissions.
    pub fn add_file_from_data_with_metadata(
        &mut self,
        archive_path: &str,
        data: &[u8],
        metadata: &crate::entry::ArchiveEntry,
    ) -> Result<()> {
        let size = data.len() as u64;
        let perm = metadata.permissions.unwrap_or(0o644);
        self.write_entry(
            archive_path,
            size,
            perm,
            metadata.modified,
            metadata.accessed,
            metadata.created,
            EntryDataSource::Bytes(data),
        )
    }

    /// Stream a file's contents into the archive without preserving source
    /// metadata.
    ///
    /// libarchive requires the entry size up front. Used by `commit_changes()`
    /// on the non-preserving path.
    pub fn add_file_from_reader<R: std::io::Read>(
        &mut self,
        archive_path: &str,
        reader: &mut R,
        size: u64,
    ) -> Result<()> {
        self.write_entry(
            archive_path,
            size,
            0o644,
            None,
            None,
            None,
            EntryDataSource::Stream {
                reader,
                expected_size: size,
            },
        )
    }

    /// Stream a file's contents into the archive while preserving metadata.
    ///
    /// libarchive requires the entry size up front, so callers must supply
    /// the known uncompressed length (available from the source entry).
    /// Used by `commit_changes()` to round-trip retained entries without
    /// buffering them fully in memory.
    pub fn add_file_from_reader_with_metadata<R: std::io::Read>(
        &mut self,
        archive_path: &str,
        reader: &mut R,
        size: u64,
        metadata: &crate::entry::ArchiveEntry,
    ) -> Result<()> {
        let perm = metadata.permissions.unwrap_or(0o644);
        self.write_entry(
            archive_path,
            size,
            perm,
            metadata.modified,
            metadata.accessed,
            metadata.created,
            EntryDataSource::Stream {
                reader,
                expected_size: size,
            },
        )
    }

    /// Add a directory entry to the archive (without contents).
    ///
    /// Stamps wall-clock mtime and `0o755`; callers that need to preserve a
    /// source directory's timestamp or mode use
    /// [`Self::add_directory_entry_with_metadata`].
    pub fn add_directory_entry(&mut self, archive_path: &str) -> Result<()> {
        self.add_directory_entry_with_metadata(archive_path, None, None)
    }

    /// Add a directory entry, preserving an optional source `mtime` and unix
    /// `mode`.
    ///
    /// `mtime` and `mode` fall back to wall-clock time and `0o755` when absent
    /// so `add_directory_entry` and the empty-source root entry stay
    /// byte-stable with prior behavior; `add_directory_recursive` passes the
    /// walked directory's metadata so recursive creation preserves directory
    /// timestamps and permissions (R0080-0070). Pre-epoch mtimes survive as
    /// signed seconds via [`systemtime_to_signed`] (R0080-0067).
    pub fn add_directory_entry_with_metadata(
        &mut self,
        archive_path: &str,
        mtime: Option<std::time::SystemTime>,
        mode: Option<u32>,
    ) -> Result<()> {
        let write_handle = self
            .write_handle
            .ok_or_else(|| ArchiveError::write_mode_only("add_directory_entry"))?;

        self.assert_not_poisoned("add_directory_entry")?;

        // R0080-0040: poll the progress/cancellation callback *before* the
        // header is written or counted. A directory entry carries zero
        // payload, so notifying up front means a `ControlFlow::Break` leaves
        // nothing committed and no counter drift — there is nothing to poison
        // or roll back. Firing `notify(0)` here also keeps per-entry progress
        // consistent with the ZIP writer's directory-entry `notify(0)`
        // (R0071-0008).
        self.notify_progress(0)?;

        let mode = mode.unwrap_or(0o755);
        let (mtime_secs, mtime_nanos) = mtime
            .map(systemtime_to_signed)
            .unwrap_or_else(|| systemtime_to_signed(std::time::SystemTime::now()));

        let result: Result<()> = (|| unsafe {
            let entry = archive_entry_new();
            if entry.is_null() {
                return Err(ArchiveError::format(None, "Failed to create entry"));
            }

            // Ensure trailing slash for directory path
            let dir_path = crate::ffi::common::ensure_trailing_slash(archive_path);

            let c_path = match CString::new(dir_path) {
                Ok(c) => c,
                Err(_) => {
                    archive_entry_free(entry);
                    return Err(ArchiveError::invalid_path(
                        archive_path,
                        "Contains null byte",
                    ));
                }
            };
            archive_entry_set_pathname(entry, c_path.as_ptr());

            archive_entry_set_filetype(entry, AE_IFDIR as c_uint);
            archive_entry_set_size(entry, 0);
            archive_entry_set_perm(entry, mode as la_mode_t);

            archive_entry_set_mtime(entry, mtime_secs, mtime_nanos as c_longlong);

            let header_result = archive_write_header(write_handle, entry);
            if header_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }

            let finish_result = archive_write_finish_entry(write_handle);
            if finish_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }
            archive_entry_free(entry);
            Ok(())
        })();

        if let Err(e) = result {
            self.write_poisoned = true;
            return Err(e);
        }

        self.entries_written += 1;
        Ok(())
    }

    /// Number of entries written so far (write mode only)
    pub fn entries_written(&self) -> usize {
        self.entries_written
    }

    /// Terminal-failure error re-reported by every finalization attempt that
    /// follows a failed one (R0001-0018).
    ///
    /// `reason` is the rendered text of the *original* failure, recorded in
    /// `finish_failure` at the single observation point in `close_write`.
    /// Replaying it is the whole point of keeping the string: the first
    /// attempt returns libarchive's own sentence ("Write error: No space left
    /// on device", a codec finalize message, …) and every later attempt must
    /// say the same thing rather than degrading to a path-only complaint.
    /// This state is deliberately independent of the entry-write
    /// `write_poisoned` flag — a poisoned writer may still finalize the
    /// entries that succeeded.
    fn finalization_failed_error(&self, reason: &str) -> ArchiveError {
        ArchiveError::operation_blocked(
            crate::error::ops::FINISH,
            format!(
                "Libarchive finalization for `{}` already failed; the archive was never durably \
                 finalized and must be discarded and rewritten: {reason}",
                self.path.display()
            ),
        )
    }

    /// Close and finalize the archive (for write mode)
    ///
    /// R0001-0018: finalization is a one-way door. `write_handle` is taken
    /// before `archive_write_close`, so a failure destroys the only state
    /// that distinguishes "never a writer / already finalized cleanly" from
    /// "tried to finalize and failed" — a retry used to fall into the
    /// empty-state success path and report `Ok(())` for an archive that was
    /// never durably finalized. The first failure therefore returns its
    /// precise typed error *and* records the rendered reason in
    /// `finish_failure`; every later attempt replays it. Success never
    /// records anything, so a repeated finish after a clean close stays
    /// `Ok(())`. Mirrors `ZipWriter::finish`.
    pub fn close_write(&mut self) -> Result<()> {
        if let Some(reason) = &self.finish_failure {
            return Err(self.finalization_failed_error(reason));
        }
        let Some(write_handle) = self.write_handle.take() else {
            // No handle left: a read-mode handle, or a writer already
            // finalized cleanly. A writer whose finalization failed was
            // caught by the replay branch above.
            return Ok(());
        };

        let result = self.finalize_handle(write_handle);
        if let Err(e) = &result {
            // Render now: `ArchiveError` is not `Clone`, and the caller
            // still receives the original typed error below. One
            // observation point covers both failure legs (the
            // `archive_write_close` message and the `sync_data` errno),
            // which is why it lives here rather than at each `return`.
            self.finish_failure = Some(e.to_string());
        }
        result
    }

    /// Finalize an already-taken libarchive write handle and make the result
    /// durable. Split out of [`LibarchiveArchive::close_write`] so the sticky
    /// failure bookkeeping (R0001-0018) has a single place to observe, the
    /// same shape `ZipWriter::finalize_writer` uses.
    fn finalize_handle(&mut self, write_handle: *mut Archive) -> Result<()> {
        unsafe {
            let close_result = archive_write_close(write_handle);
            // Capture the libarchive error string *before* freeing
            // the handle (R0070-0043). Releasing first turned every
            // close failure into the catch-all "Failed to close
            // archive" message, hiding the real reason (disk full,
            // codec finalize failure, …).
            let close_error_msg = if close_result != ARCHIVE_OK {
                Some(get_archive_error(write_handle))
            } else {
                None
            };
            archive_write_free(write_handle);
            if let Some(msg) = close_error_msg {
                // R0001-0018: libarchive is done with the descriptor, so
                // release `write_output` here instead of stranding an
                // unsynced output behind the error. Deliberately *not*
                // `sync_data` — the bytes on disk are a partial archive and
                // making them durable would only harden a corrupt file;
                // `finish_failure`, recorded by the caller, is what keeps
                // the failure reportable.
                drop(self.write_output.take());
                return Err(ArchiveError::io(
                    "close",
                    self.path.clone(),
                    std::io::Error::other(msg),
                ));
            }
        }
        // R0079-0002: durability boundary, mirroring
        // `ZipWriter::finish`. `archive_write_close` flushes
        // libarchive's buffers into the fd but never fsyncs, so
        // without this a crash right after `finish()` — or after
        // `commit_changes`'s rename swap publishes the temp
        // archive — could lose the data while keeping the
        // directory entry. `write_output` is held on Unix only
        // (see the R0075-0028 ownership note in `create`).
        if let Some(output) = self.write_output.take() {
            // R0001-0018: a failed `sync_data` is also a failed
            // finalization — the `?` propagates to `close_write`, which
            // records the reason, so the retry reports the terminal state
            // instead of `Ok(())`.
            output
                .sync_data()
                .map_err(|e| ArchiveError::io(crate::error::ops::CREATE, self.path.clone(), e))?;
        }
        // Best-effort parent-directory sync — the file's data is
        // already durable; directory-entry visibility is a hint,
        // not a correctness gate (R0076-0013 policy).
        let _ = crate::ffi::common::sync_parent_dir(&self.path);
        Ok(())
    }

    /// Add a file from filesystem path to the archive.
    ///
    /// Symlinks are rejected up-front so the single-file path matches
    /// `add_directory_recursive`'s loud-rejection policy instead of
    /// silently following the link and archiving the target's bytes
    /// under the link's name (R0069-0055).
    pub fn add_file_from_path(
        &mut self,
        fs_path: impl AsRef<Path>,
        archive_path: &str,
    ) -> Result<()> {
        // R0076-0014: see the ZIP writer's twin call site — open and
        // re-verify in one helper instead of check-then-open.
        let (mut file, metadata) = crate::ffi::common::open_file_no_follow_symlinks(
            fs_path.as_ref(),
            "add_file_from_path",
        )?;

        let size = metadata.len();
        // R0081-0060: `modified()`, `accessed()`, and `created()` are all
        // best-effort here. On the supported targets each only returns
        // `Err` — mapped to `None` by `.ok()` — when the platform's
        // filesystem does not expose that timestamp field: `created()`
        // (birthtime) is absent on many Linux filesystems, and the atime
        // getters can be too. The entry simply omits the unsupported
        // field rather than failing the whole add. Hard-propagating a
        // required-field failure is a behaviour decision left out of
        // scope.
        let mtime = metadata.modified().ok();
        let atime = metadata.accessed().ok();
        let btime = metadata.created().ok();

        #[cfg(unix)]
        let perm = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode()
        };
        #[cfg(not(unix))]
        let perm = 0o644u32;

        self.write_entry(
            archive_path,
            size,
            perm,
            mtime,
            atime,
            btime,
            EntryDataSource::Stream {
                reader: &mut file,
                expected_size: size,
            },
        )
    }

    fn assert_not_poisoned(&self, op: &'static str) -> Result<()> {
        if self.write_poisoned {
            return Err(ArchiveError::operation_blocked(
                op,
                "Libarchive writer is poisoned after a prior entry-write failure; \
                 call finish() to drain or drop the writer to discard",
            ));
        }
        Ok(())
    }

    /// Shared backend for the four `add_file_*` methods.
    ///
    /// Sets up a regular-file entry with the given size/perm/mtime, writes the
    /// header, streams the data from `source`, and finalizes the entry. The
    /// entry pointer is freed via RAII so any `?`-propagated error is leak-safe.
    /// Progress / cancellation fire per-chunk (R0071-0007) so a streaming
    /// `ControlFlow::Break` no longer waits for the whole entry to finish.
    #[allow(clippy::too_many_arguments)]
    fn write_entry(
        &mut self,
        archive_path: &str,
        size: u64,
        perm: u32,
        mtime: Option<std::time::SystemTime>,
        atime: Option<std::time::SystemTime>,
        btime: Option<std::time::SystemTime>,
        source: EntryDataSource<'_>,
    ) -> Result<()> {
        let write_handle = self
            .write_handle
            .ok_or_else(|| ArchiveError::write_mode_only("write_entry"))?;

        self.assert_not_poisoned("write_entry")?;

        // Lazily allocate the streaming scratch buffer the first time we need it.
        if matches!(source, EntryDataSource::Stream { .. }) && self.stream_buffer.is_empty() {
            self.stream_buffer.resize(64 * 1024, 0);
        }

        // Borrow split: stream_buffer / progress / bytes_written are
        // independent fields, so the unsafe inner call can hold the
        // first while the notify closure mutates the other two.
        let stream_buffer = &mut self.stream_buffer;
        let progress_ref = &mut self.progress;
        let bytes_written_ref = &mut self.bytes_written;
        let mut notify = |chunk: u64| -> Result<()> {
            crate::ffi::common::notify_creation_progress(progress_ref, bytes_written_ref, chunk)
        };

        let result = unsafe {
            Self::write_entry_inner(
                write_handle,
                stream_buffer,
                archive_path,
                size,
                perm,
                mtime,
                atime,
                btime,
                source,
                &mut notify,
            )
        };

        match result {
            Ok(_) => {
                self.entries_written += 1;
                Ok(())
            }
            Err(e) => {
                self.write_poisoned = true;
                Err(e)
            }
        }
    }

    /// FFI-only inner half of `write_entry`. All libarchive calls live here so
    /// the borrow of `self` stays narrow and the public wrapper can update
    /// progress fields after we drop the entry.
    ///
    /// `notify` is called once for `Bytes` sources (`data.len()` bytes) and
    /// per-chunk for `Stream` sources, so a streaming progress callback
    /// returning `ControlFlow::Break` cancels the in-progress entry rather
    /// than waiting for the whole copy to finish (R0071-0007).
    #[allow(clippy::too_many_arguments)]
    unsafe fn write_entry_inner(
        write_handle: *mut Archive,
        stream_buffer: &mut [u8],
        archive_path: &str,
        size: u64,
        perm: u32,
        mtime: Option<std::time::SystemTime>,
        atime: Option<std::time::SystemTime>,
        btime: Option<std::time::SystemTime>,
        source: EntryDataSource<'_>,
        notify: &mut dyn FnMut(u64) -> Result<()>,
    ) -> Result<u64> {
        let guard = EntryGuard::new()?;
        let entry = guard.entry;

        let c_path = CString::new(archive_path)
            .map_err(|_| ArchiveError::invalid_path(archive_path, "Contains null byte"))?;
        // Guard against u64-to-c_longlong wraparound. Sizes above
        // i64::MAX would silently become negative, leading libarchive to
        // reject the entry or write a malformed header (R0069-0046).
        if size > c_longlong::MAX as u64 {
            return Err(ArchiveError::operation_blocked(
                crate::error::ops::CREATE,
                format!(
                    "Entry size {} bytes exceeds libarchive's signed-size limit ({} bytes)",
                    size,
                    c_longlong::MAX
                ),
            ));
        }
        unsafe {
            archive_entry_set_pathname(entry, c_path.as_ptr());
            archive_entry_set_filetype(entry, AE_IFREG as c_uint);
            archive_entry_set_size(entry, size as c_longlong);
            archive_entry_set_perm(entry, perm as la_mode_t);

            // Pre-epoch timestamps map to negative seconds instead of being
            // replaced with `now()` (R0080-0067). The no-mtime case still
            // falls back to wall-clock time so entries without a source
            // timestamp get a sensible value.
            let (mtime_secs, mtime_nanos) = mtime
                .map(systemtime_to_signed)
                .unwrap_or_else(|| systemtime_to_signed(std::time::SystemTime::now()));
            archive_entry_set_mtime(entry, mtime_secs, mtime_nanos as c_longlong);

            // OI-0065-002: emit access and birth (creation) timestamps
            // when available so a `commit_changes()` round-trip with
            // `preserve_metadata=true` no longer drops them. Each is
            // optional — only set the libarchive field when the source
            // provided a value, so unknown timestamps stay unknown
            // rather than getting backfilled with `now()`. Pre-epoch
            // values survive as signed seconds instead of being dropped
            // (R0080-0068).
            if let Some(t) = atime {
                let (secs, nanos) = systemtime_to_signed(t);
                archive_entry_set_atime(entry, secs, nanos as c_longlong);
            }
            if let Some(t) = btime {
                let (secs, nanos) = systemtime_to_signed(t);
                archive_entry_set_birthtime(entry, secs, nanos as c_longlong);
            }

            if archive_write_header(write_handle, entry) != ARCHIVE_OK {
                return Err(ArchiveError::format(None, get_archive_error(write_handle)));
            }
        }

        let written = match source {
            EntryDataSource::Bytes(data) => {
                if !data.is_empty() {
                    let n = unsafe {
                        archive_write_data(write_handle, data.as_ptr() as *const c_void, data.len())
                    };
                    if n < 0 {
                        let msg = unsafe { get_archive_error(write_handle) };
                        return Err(ArchiveError::io(
                            "write_data",
                            archive_path,
                            std::io::Error::other(msg),
                        ));
                    }
                    if n as usize != data.len() {
                        return Err(ArchiveError::io(
                            "write_data",
                            archive_path,
                            std::io::Error::other(format!(
                                "Short write: requested {} bytes, wrote {} bytes",
                                data.len(),
                                n
                            )),
                        ));
                    }
                }
                // Always notify (even for empty data) so per-entry
                // progress callbacks fire (R0071-0007).
                notify(data.len() as u64)?;
                size
            }
            EntryDataSource::Stream {
                reader,
                expected_size,
            } => {
                // `take` is a `Sized`-only `Read` method, so the trait must be
                // in scope to invoke it on the `&mut dyn Read` receiver.
                use std::io::Read as _;
                let mut total: u64 = 0;
                // Bound the reader to one byte past the declared size so an
                // over-producing (growing or malicious) source is caught on
                // the first extra byte instead of after reading to EOF
                // (R0080-0037). libarchive was told `expected_size` up front,
                // so the surplus byte must never reach `archive_write_data`.
                let mut bounded = reader.take(expected_size.saturating_add(1));
                loop {
                    let n = match bounded.read(stream_buffer) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) => {
                            return Err(ArchiveError::io("read_stream", archive_path, e));
                        }
                    };
                    // Checked accounting + immediate overproduction guard: the
                    // extra byte from `take` pushes `total` past
                    // `expected_size`, so fail before writing it out.
                    total = total.checked_add(n as u64).ok_or_else(|| {
                        ArchiveError::io(
                            "write_data",
                            archive_path,
                            std::io::Error::other("Stream length counter overflow"),
                        )
                    })?;
                    if total > expected_size {
                        // DCR-011: a source that hands the writer a byte
                        // count other than the size it declared is a
                        // declared-size violation, not an I/O fault, so
                        // every commit route classifies it the same way
                        // through the shared constructor. `total` is a
                        // lower bound here — the `take(expected + 1)`
                        // probe byte stops the copy instead of draining
                        // the source — which is why the rendered message
                        // says "at least".
                        return Err(ArchiveError::declared_length_mismatch(
                            archive_path,
                            expected_size,
                            total,
                        ));
                    }
                    let written = unsafe {
                        archive_write_data(write_handle, stream_buffer.as_ptr() as *const c_void, n)
                    };
                    if written < 0 {
                        let msg = unsafe { get_archive_error(write_handle) };
                        return Err(ArchiveError::io(
                            "write_data",
                            archive_path,
                            std::io::Error::other(msg),
                        ));
                    }
                    if written as usize != n {
                        return Err(ArchiveError::io(
                            "write_data",
                            archive_path,
                            std::io::Error::other(format!(
                                "Short write: requested {} bytes, wrote {} bytes",
                                n, written
                            )),
                        ));
                    }
                    // Per-chunk progress / cancellation (R0071-0007).
                    notify(n as u64)?;
                }
                if total != expected_size {
                    // Under-production (the reader hit EOF early); same
                    // DCR-011 classification as the over-production
                    // guard above.
                    return Err(ArchiveError::declared_length_mismatch(
                        archive_path,
                        expected_size,
                        total,
                    ));
                }
                // Empty-stream guard: still fire one per-entry callback
                // so progress sees the entry boundary.
                if total == 0 {
                    notify(0)?;
                }
                total
            }
        };

        if unsafe { archive_write_finish_entry(write_handle) } != ARCHIVE_OK {
            return Err(ArchiveError::format(None, unsafe {
                get_archive_error(write_handle)
            }));
        }

        drop(guard);
        Ok(written)
    }

    /// Add directory recursively to archive
    pub fn add_directory_recursive(&mut self, dir_path: impl AsRef<Path>) -> Result<()> {
        use crate::ffi::common::{DirWalkKind, walk_directory_tree};

        let dir_path = dir_path.as_ref();

        let mut emitted_any = false;
        walk_directory_tree(dir_path, "walk", |item| {
            if item.is_root {
                return Ok(());
            }
            match item.kind {
                DirWalkKind::File => {
                    self.add_file_from_path(item.fs_path, &item.archive_path)?;
                    emitted_any = true;
                }
                DirWalkKind::Dir => {
                    if item.is_leaf_dir {
                        // Preserve the source directory's mtime and mode so
                        // recursive creation is deterministic and
                        // metadata-preserving (R0080-0070).
                        let md = std::fs::metadata(item.fs_path)
                            .map_err(|e| ArchiveError::io("metadata", item.fs_path, e))?;
                        // R0081-0061: the directory mtime is best-effort —
                        // `.ok()` drops it to `None` only on a platform whose
                        // filesystem does not expose a modification time, in
                        // which case the entry is written without one rather
                        // than failing the recursive add.
                        let mtime = md.modified().ok();
                        #[cfg(unix)]
                        let mode = {
                            use std::os::unix::fs::PermissionsExt;
                            Some(md.permissions().mode())
                        };
                        #[cfg(not(unix))]
                        let mode = None;
                        self.add_directory_entry_with_metadata(&item.archive_path, mtime, mode)?;
                        emitted_any = true;
                    }
                }
                // ZIP rejects symlinks/special files loudly; the libarchive
                // path used to silently skip them, producing archives that
                // differ in contents by backend. Reject with a structured
                // error so callers see consistent behavior.
                DirWalkKind::Special { .. } => {
                    return Err(ArchiveError::invalid_path(
                        item.fs_path.to_string_lossy().as_ref(),
                        "unsupported entry kind (symlinks and special files are \
                         rejected to keep recursive-create parity across backends)",
                    ));
                }
            }
            Ok(())
        })?;

        // R0070-0047: empty source directory still produces a single
        // root entry (matches the ZIP backend).
        if !emitted_any {
            let root_name = dir_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !root_name.is_empty() {
                self.add_directory_entry(&root_name)?;
            }
        }

        Ok(())
    }
}

/// R0001-0018: `close_write` is the finalization state machine — a failed
/// finalization must stay terminal for every later attempt, while the
/// no-handle success path (read-mode handles, a clean second finish) keeps
/// returning `Ok(())`. These tests drive the state directly because a real
/// `archive_write_close` failure needs a full disk or a broken codec.
#[cfg(test)]
mod close_write_state_tests {
    use crate::ffi::libarchive_wrapper::LibarchiveArchive;
    use once_cell::sync::OnceCell;
    use std::path::Path;

    /// Build a handle in the post-finalization state: the write handle has
    /// already been taken, and `finish_failure` carries the rendered reason
    /// of that attempt's failure (or `None` when it succeeded / never ran).
    fn finalized_handle(path: &Path, finish_failure: Option<&str>) -> LibarchiveArchive {
        LibarchiveArchive {
            path: path.to_path_buf(),
            write_handle: None,
            write_output: None,
            progress: None,
            bytes_written: 0,
            entries_written: 0,
            stream_buffer: Vec::new(),
            write_poisoned: false,
            finish_failure: finish_failure.map(str::to_string),
            cached_listing: OnceCell::new(),
            identity: None,
            backend_warnings: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// A finish that follows a failed finish must report the terminal
    /// failure — never `Ok(())` for an archive that was never durably
    /// finalized — and must keep doing so on every further attempt.
    ///
    /// R0001-0018: the replay also has to carry libarchive's *original*
    /// sentence. The seeded reason is exactly what `close_write` records:
    /// the `Display` of the `ArchiveError::Io` built from
    /// `get_archive_error`'s capture-before-free. Before the
    /// `finish_failure` split that text existed only inside the first
    /// returned error and every later attempt degraded to naming the path.
    #[test]
    fn finish_after_failed_finalization_stays_terminal() {
        let mut archive = finalized_handle(
            Path::new("/nonexistent/failed.tar"),
            Some(
                "I/O error during close: /nonexistent/failed.tar (Write error: No space left on device)",
            ),
        );

        for attempt in 0..3 {
            let err = archive
                .close_write()
                .expect_err("finalization failed once; every later finish must report it");
            let msg = err.to_string();
            assert!(
                msg.contains("already failed") && msg.contains("failed.tar"),
                "attempt {attempt}: expected the terminal finalization error, got: {msg}"
            );
            assert!(
                msg.contains("No space left on device"),
                "attempt {attempt}: the replay must preserve libarchive's original failure \
                 text, got: {msg}"
            );
        }
    }

    /// The empty-state success path is preserved for handles that never
    /// finalized anything: read-mode handles and a repeat finish after a
    /// clean close.
    #[test]
    fn finish_without_pending_handle_is_ok() {
        let mut archive = finalized_handle(Path::new("/nonexistent/fine.tar"), None);
        archive
            .close_write()
            .expect("a handle with nothing to finalize stays a no-op success");
    }

    /// The entry-write poison (R0072-0008) and the terminal finalization
    /// state (R0001-0018) are separate: a writer poisoned by a failed
    /// entry write must still finalize the entries that succeeded, and a
    /// repeated finish after that stays `Ok(())`. Re-conflating the two
    /// flags fails here.
    #[test]
    fn entry_write_poison_does_not_make_finalization_terminal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("poisoned.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");
        writer.add_file_from_data("a.txt", b"alpha").expect("add a");

        // Declare 4 bytes and feed far more: the overproduction check
        // fails the entry write and poisons the writer.
        let mut reader = std::io::Cursor::new(vec![b'x'; 70_000]);
        writer
            .add_file_from_reader("big.txt", &mut reader, 4)
            .expect_err("overproduction must be rejected");
        assert!(
            writer.write_poisoned,
            "a failed entry write must poison the writer"
        );
        assert!(
            writer.finish_failure.is_none(),
            "an entry-write failure is not a finalization failure"
        );

        writer
            .close_write()
            .expect("an entry-write poison must not block draining the entries that succeeded");
        writer
            .close_write()
            .expect("a second finish after a successful one stays Ok");
    }

    /// Success is the transition into the closed-and-fine state: a real
    /// create/close leaves the handle unpoisoned, so a second finish (as the
    /// facade's `Drop` performs after an explicit `finish`) is still `Ok`.
    #[test]
    fn repeated_finish_after_successful_close_is_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("finished.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");
        writer.add_file_from_data("a.txt", b"alpha").expect("add a");
        writer.close_write().expect("finish");
        assert!(
            writer.finish_failure.is_none(),
            "a clean close must leave no terminal-failure reason recorded"
        );
        assert!(
            !writer.write_poisoned,
            "a clean run leaves no entry-write poison either"
        );
        writer
            .close_write()
            .expect("a second finish after a successful one stays Ok");
    }
}

/// The two classification decisions this module owns: a declared-size
/// violation is `Corruption` (DCR-011), and a missing *write* filter is
/// `CodecUnavailable` with install instructions rather than a generic
/// `Format` error.
#[cfg(test)]
mod classification_tests {
    use super::codec_unavailable_with_detail;
    use crate::error::ArchiveError;
    use crate::ffi::libarchive_wrapper::LibarchiveArchive;

    /// A source that produces more than it declared is a declared-size
    /// violation, not an I/O fault: DCR-011 already classifies those as
    /// `Corruption` on the digest routes, so the write routes must agree.
    #[test]
    fn overproducing_stream_is_corruption_naming_the_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("overproduce.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");

        let mut reader = std::io::Cursor::new(vec![b'x'; 70_000]);
        let err = writer
            .add_file_from_reader("big.txt", &mut reader, 4)
            .expect_err("overproduction must be rejected");

        match &err {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "big.txt", "the entry name must survive: {err}");
                assert!(
                    details.contains("over-produced"),
                    "the direction must be stated: {details}"
                );
                assert!(
                    details.contains('4'),
                    "the declared size must be stated: {details}"
                );
            }
            other => panic!("declared-size violations are Corruption, got {other:?}"),
        }
    }

    /// The mirror case: a reader that hits EOF early. Same variant, the
    /// other direction.
    #[test]
    fn underproducing_stream_is_corruption_naming_the_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("underproduce.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");

        let mut reader = std::io::Cursor::new(b"ab".to_vec());
        let err = writer
            .add_file_from_reader("short.txt", &mut reader, 64)
            .expect_err("underproduction must be rejected");

        match &err {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "short.txt", "the entry name must survive: {err}");
                assert!(
                    details.contains("under-produced"),
                    "the direction must be stated: {details}"
                );
            }
            other => panic!("declared-size violations are Corruption, got {other:?}"),
        }
    }

    /// A failed write-filter registration is a deployment problem the
    /// caller can act on, so it carries the codec name and the
    /// per-platform install hint the error module maintains. Driven
    /// directly because a real failure needs a libarchive built without
    /// the codec.
    #[test]
    fn failed_write_filter_registration_maps_to_codec_unavailable() {
        for codec in ["zstd", "lz4", "lzma", "xz", "bzip2", "gzip"] {
            let err = codec_unavailable_with_detail(
                codec,
                crate::ArchiveFormat::Tar,
                "Unsupported compression",
            );
            match &err {
                ArchiveError::CodecUnavailable {
                    codec: reported,
                    format,
                    install_instructions,
                } => {
                    assert_eq!(reported, codec, "the codec must be named");
                    assert_eq!(*format, crate::ArchiveFormat::Tar);
                    assert!(
                        install_instructions.contains("Unsupported compression"),
                        "libarchive's own reason must survive: {install_instructions}"
                    );
                    assert!(
                        !install_instructions.trim().is_empty(),
                        "install instructions must be populated"
                    );
                }
                other => panic!("expected CodecUnavailable for {codec}, got {other:?}"),
            }
        }
    }

    /// The lower-case libarchive filter names must reach the real
    /// per-platform arms of the install table, not the generic
    /// fallback — that is the whole point of naming the codec.
    #[test]
    fn libarchive_filter_names_reach_the_real_install_instructions() {
        for codec in ["xz", "lzma", "bzip2"] {
            let generic = format!("Install {codec} codec for your platform");
            let err = codec_unavailable_with_detail(codec, crate::ArchiveFormat::TarXz, "");
            let msg = err.to_string();
            assert!(
                !msg.contains(&generic),
                "{codec} must not fall back to the generic hint: {msg}"
            );
            assert!(
                msg.contains(codec),
                "the caller's spelling must be preserved: {msg}"
            );
        }
    }

    /// With no native detail there is no trailing parenthetical — the
    /// message stays the plain install instruction.
    #[test]
    fn empty_native_detail_leaves_the_instructions_alone() {
        let with_detail =
            codec_unavailable_with_detail("zstd", crate::ArchiveFormat::TarZst, "   ");
        match with_detail {
            ArchiveError::CodecUnavailable {
                install_instructions,
                ..
            } => assert!(
                !install_instructions.contains("libarchive reported"),
                "blank detail must not be appended: {install_instructions}"
            ),
            other => panic!("expected CodecUnavailable, got {other:?}"),
        }
    }

    /// Default behaviour is unchanged: on a libarchive that *does* carry
    /// the filters, creating each filtered format still succeeds.
    #[test]
    fn filtered_formats_still_construct_on_a_complete_libarchive() {
        for (format, name) in [
            (crate::ArchiveFormat::TarGzip, "a.tar.gz"),
            (crate::ArchiveFormat::TarBzip2, "a.tar.bz2"),
            (crate::ArchiveFormat::TarXz, "a.tar.xz"),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join(name);
            let mut options = crate::options::CompressionOptions::default();
            let mut writer = LibarchiveArchive::create(&path, format, &mut options)
                .unwrap_or_else(|e| panic!("create {format:?}: {e}"));
            writer.add_file_from_data("a.txt", b"alpha").expect("add");
            writer.close_write().expect("finish");
        }
    }
}
