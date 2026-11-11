//! glibc allocator settings for a long-running client that decodes media on many threads.
//! Other platforms and C libraries keep their defaults.

/// Applied once at startup, before worker threads exist. Each GStreamer streaming thread
/// otherwise gets its own arena, and glibc's dynamic mmap threshold moves multi-megabyte
/// video frames into those arenas once one is freed; every arena then keeps tens of MB of
/// freed frames resident. A fixed 1 MiB threshold maps such buffers separately, so freeing
/// them returns the memory, and four arenas bound what the remaining heaps can keep.
///
/// Allocations just under that threshold (decoded images, network and decoder buffers) still
/// coalesce into multi-megabyte free holes that glibc only returns from an arena's top, so a
/// background thread trims periodically. Transparent huge pages are disabled for the process:
/// with the kernel's `always` mode a single live chunk keeps its whole 2 MiB page resident.
#[allow(unsafe_code)]
pub fn tune_allocator() {}

/// Returns freed allocator memory to the system, such as once playback or a poster batch ends.
/// Elsewhere this does nothing.
#[allow(unsafe_code)]
pub fn release_freed_memory() {}
