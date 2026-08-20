# How-to guides for the user audience

* [How to add, replace, or remove entries in an existing archive](modify-a-zip-or-7z.md) - Queue additions, replacements, and removals against an existing ZIP or 7z archive and commit them as one atomic rewrite.
* [How to create an archive in a given format](create-an-archive.md) - Pick a creatable format, build the right options value, add entries, finalize the writer, and recognise the refusals this recipe runs into.
* [How to detect and open a self-extracting archive](handle-self-extracting-archives.md) - Detect an SFX executable, read the detection verdict, open or stage its embedded payload, and pull the stub bytes out for inspection.
* [How to extract a multi-part RAR set](extract-multi-part-rar-sets.md) - Opens the right volume of a split RAR set, confirms the set with detect_multipart, and extracts it in one call.
* [How to extract an untrusted archive safely](extract-untrusted-archives-safely.md) - Configure extraction limits, an overwrite policy, and checksum verification before extracting an archive from an untrusted source, then inspect the warnings the run returns.
* [How to open password-protected archives](open-password-protected-archives.md) - Detect that an archive needs a password, open it with one, pass it through extraction options, and read the failure when it is wrong.
* [How to pick the right extraction call](choose-an-extraction-api.md) - Routes an extraction goal to the matching Archive method and shows the two selection choices that are easy to get wrong.
* [How to report progress and cancel an operation](report-progress-and-cancel.md) - Wire a ProgressCallback into extraction, creation, and SFX staging, throttle it, and understand what a cancellation leaves on disk.
* [How to stream a large entry](stream-a-large-entry.md) - Read one archive entry incrementally through a bounded StreamingExtractor and pick the StreamBound that matches your trust in the archive.
* [How to verify an archive's integrity](verify-archive-integrity.md) - Recipes for validating an archive with validate_integrity, verify_crc32, the standalone stream-checksum helpers, and the content digests.

Full bundle: [Manual index](../../../index.md)
