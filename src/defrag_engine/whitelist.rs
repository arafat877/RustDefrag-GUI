/// whitelist.rs — Skip frequently re-fragmented Microsoft system binaries.
/// MRT.exe (Malicious Removal Tool) is updated monthly by Windows Update
/// and is a known heavy re-fragmenter.

use std::path::Path;

/// Returns true if the file is on the whitelist and should be skipped.
pub fn is_whitelisted(path: &Path) -> bool {
    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    WHITELIST.iter().any(|&w| name == w)
}

/// Files that are updated by Windows Update so frequently that defragging
/// them wastes more time than it saves. They will be re-fragmented within
/// hours. Let Windows manage their placement.
static WHITELIST: &[&str] = &[
    // Windows Malicious Software Removal Tool — updated every Patch Tuesday
    "mrt.exe",
    "mrtstub.exe",
    // Windows Update delivery optimization cache
    "doinst.exe",
    // Windows Defender signature database — updated multiple times/day
    "mpengine.dll",
    "mpasbase.vdm",
    "mpasdlta.vdm",
    "mpavbase.vdm",
    "mpavdlta.vdm",
    // NTFS change journal — continuously written by the OS
    "$usnjrnl",
    // Event log files — continuously appended
    "system.evtx",
    "application.evtx",
    "security.evtx",
    // Browser caches — extremely volatile
    "webdata",
    "cache",
    "cookies",
];
