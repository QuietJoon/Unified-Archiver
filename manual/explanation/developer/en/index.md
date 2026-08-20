# Explanation for the developer audience

* [How SFX detection decides](sfx-detection-pipeline.md) - Why self-extracting-archive detection is staged, why its verdict is an enum with an evidence trail, and what it refuses to claim.
* [How this project records decisions](decision-records-and-reviews.md) - The append-only decision store under docs/records, the review gates that feed it, the open-issues ledger, and how code points back at the ruling that authorised it.
* [Streaming and memory behaviour](streaming-and-memory.md) - Why one entry-reader type covers backends with very different memory profiles, and why exceeding the stream's cap is an error instead of a silent end of data.
* [Why modification rewrites the archive](modification-is-a-rewrite.md) - The reasoning behind copy-on-write modification, the advisory lock and inode-identity guard that protect it, and why only ZIP and 7z are in scope.

Full bundle: [Manual index](../../../index.md)
