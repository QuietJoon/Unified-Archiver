# Explanation for the user audience

* [One API over many backends](one-api-many-backends.md) - Why unified-archive puts a single Archive type over an internal backend enum, how formats map to engines, and which asymmetries the facade refuses to hide.
* [The extraction safety model](extraction-safety-model.md) - What unified-archive defends against when it extracts an archive, why the gates run before the first byte is written, and what the model deliberately leaves to you.
* [What archive checksums actually prove](checksums-and-integrity.md) - Discussion of the three kinds of checksum this crate exposes, what each one detects, and where the guarantees stop.

Full bundle: [Manual index](../../../index.md)
