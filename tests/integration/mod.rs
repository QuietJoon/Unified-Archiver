//! Integration tests for unified-archive

#[path = "../common/mod.rs"]
pub mod common;

pub mod backend_caching_baseline;
pub mod compression_builder_split;
pub mod concurrency;
pub mod creation;
pub mod digest_ae2_duplicate_path;
pub mod digest_duplicate_path;
pub mod directory_entry_single_file;
pub mod entry_builder;
pub mod entry_filter_fnmut;
pub mod extraction;
pub mod format_compatibility;
pub mod hardlink_skip;
pub mod link_skip_single_file;
pub mod manifest_digest_perf;
pub mod modification;
pub mod multipart_typed;
pub mod non_utf8_paths;
pub mod performance_baseline;
pub mod permissions_contract;
pub mod readonly_codec_formats;
pub mod selective_extraction_link_warnings;
pub mod sfx_detection;
pub mod sfx_false_positives;
pub mod sfx_in_place_open;
pub mod sfx_staging_progress;
pub mod single_entry_defense_parity;
pub mod streaming_memory;
pub mod zip_extended_timestamps;
