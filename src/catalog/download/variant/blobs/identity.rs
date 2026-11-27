//! What a file is, as its file system tells it apart: its device and
//! inode, which every hard link of it shares, and when it last changed.

use std::{fs, io, path::Path, time::SystemTime};

/// A file's device and inode, which every hard link of it shares.
pub(in crate::catalog::download::variant) type Identity = (u64, u64);

/// The metadata of the file `path` names, links followed.
pub(super) fn metadata(path: &Path) -> io::Result<fs::Metadata> {
    #[cfg(all(test, unix))]
    count((1, 0, 0));
    fs::metadata(path)
}

/// The file `path` names, followed if a link, as its device and inode.
pub(in crate::catalog::download::variant) fn identity(path: &Path) -> Option<Identity> {
    identity_of(&metadata(path).ok()?)
}

#[cfg(unix)]
#[expect(clippy::unnecessary_wraps, reason = "off Unix no file has an identity")]
pub(in crate::catalog::download::variant) fn identity_of(
    metadata: &fs::Metadata,
) -> Option<Identity> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
pub(in crate::catalog::download::variant) const fn identity_of(
    _metadata: &fs::Metadata,
) -> Option<Identity> {
    None
}

/// Whether the file `metadata` describes changed after `time`, as a link
/// made to it changes it. A file system stamps the change to its own
/// granule, which rounds it down: a tick of the kernel's clock on most,
/// a second on HFS+ and some NFS servers, two on FAT. A change within that
/// granule of `time` reads as before it.
#[cfg(unix)]
pub(super) fn changed_after(metadata: &fs::Metadata, time: SystemTime) -> bool {
    use std::os::unix::fs::MetadataExt;
    let seconds = u64::try_from(metadata.ctime()).ok();
    let nanos = u32::try_from(metadata.ctime_nsec()).ok();
    seconds.zip(nanos).is_some_and(|(seconds, nanos)| {
        SystemTime::UNIX_EPOCH + std::time::Duration::new(seconds, nanos) > time
    })
}

#[cfg(not(unix))]
pub(super) const fn changed_after(_metadata: &fs::Metadata, _time: SystemTime) -> bool {
    false
}

#[cfg(all(test, unix))]
thread_local! {
    /// The metadata reads, `blobs` listings and snapshot walks of this
    /// thread, for tests.
    pub(in crate::catalog::download::variant) static READS:
        std::cell::Cell<(usize, usize, usize)> = const { std::cell::Cell::new((0, 0, 0)) };
}

#[cfg(all(test, unix))]
pub(super) fn count((metadata, listings, walks): (usize, usize, usize)) {
    READS.with(|reads| {
        let (read, listed, walked) = reads.get();
        reads.set((read + metadata, listed + listings, walked + walks));
    });
}
