use crate::defrag_engine::{errors::DefragResult, volume::FileEntry, winapi::{self, ClusterRun}};
use log::debug;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, atomic::{AtomicU64, AtomicUsize, Ordering}};

#[derive(Debug, Clone)]
pub struct FileFragInfo {
    pub path:           PathBuf,
    pub size_bytes:     u64,
    pub fragment_count: u32,
    pub runs:           Vec<ClusterRun>,
    pub is_fragmented:  bool,
}

impl FileFragInfo {
    pub fn total_clusters(&self) -> i64 { self.runs.iter().map(|r| r.length).sum() }
}

#[derive(Debug, Default, Clone)]
pub struct FragmentationReport {
    pub total_files:         u64,
    pub fragmented_files:    u64,
    pub total_fragments:     u64,
    pub total_clusters_used: i64,
    pub skipped_system:      u64,
    pub worst_file:          Option<FileFragInfo>,
    pub fragmented:          Vec<FileFragInfo>,
    /// Fragment count histogram: index = fragment count (capped at 50), value = file count
    pub frag_histogram:      Vec<u64>,
    /// Size distribution buckets: (<1MB, 1-10MB, 10-100MB, >100MB)
    pub size_buckets:        [u64; 4],
}

impl FragmentationReport {
    pub fn average_fragments(&self) -> f64 {
        if self.total_files == 0 { 1.0 } else { self.total_fragments as f64 / self.total_files as f64 }
    }
    pub fn fragmentation_percent(&self) -> f64 {
        if self.total_files == 0 { 0.0 } else { self.fragmented_files as f64 / self.total_files as f64 * 100.0 }
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisProgressTick {
    pub done: usize,
    pub total: usize,
    pub frag_so_far: u64,
    pub new_fragmented_runs: Vec<ClusterRun>,
}

pub fn analyse_files<F>(
    entries: &[FileEntry],
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    on_progress: F,
) -> DefragResult<FragmentationReport>
where F: Fn(AnalysisProgressTick) + Send + Sync,
{
    let total = entries.len();
    let done  = Arc::new(AtomicUsize::new(0));
    let frag_so_far = Arc::new(AtomicU64::new(0));
    let file_infos: Vec<Option<FileFragInfo>> = analysis_pool().install(|| {
        entries
            .par_iter()
            .map(|entry| {
                if stop_flag.load(Ordering::Relaxed) { return None; }
                let result = if entry.is_system || entry.is_temp { None }
                             else { analyse_single(entry) };

                let mut new_fragmented_runs = Vec::new();
                let mut frag_inc = 0u64;
                if let Some(info) = &result {
                    if info.is_fragmented {
                        new_fragmented_runs = info.runs.clone();
                        frag_inc = info.fragment_count as u64;
                    }
                }

                let frag_total = if frag_inc > 0 {
                    frag_so_far.fetch_add(frag_inc, Ordering::Relaxed) + frag_inc
                } else {
                    frag_so_far.load(Ordering::Relaxed)
                };
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                on_progress(AnalysisProgressTick {
                    done: n,
                    total,
                    frag_so_far: frag_total,
                    new_fragmented_runs,
                });
                result
            })
            .collect()
    });

    let mut rep = FragmentationReport {
        frag_histogram: vec![0u64; 51],
        ..Default::default()
    };
    for opt in file_infos.into_iter().flatten() {
        rep.total_files      += 1;
        rep.total_fragments  += opt.fragment_count as u64;
        rep.total_clusters_used += opt.total_clusters();
        let hbucket = (opt.fragment_count as usize).min(50);
        rep.frag_histogram[hbucket] += 1;
        let sb = match opt.size_bytes {
            s if s < 1_000_000   => 0,
            s if s < 10_000_000  => 1,
            s if s < 100_000_000 => 2,
            _                    => 3,
        };
        rep.size_buckets[sb] += 1;
        if opt.is_fragmented {
            rep.fragmented_files += 1;
            let worse = rep.worst_file.as_ref()
                .map(|w| opt.fragment_count > w.fragment_count).unwrap_or(true);
            if worse { rep.worst_file = Some(opt.clone()); }
            rep.fragmented.push(opt);
        }
    }
    // Sort: largest files first (better perf per the recommendations)
    rep.fragmented.sort_unstable_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    Ok(rep)
}

fn analysis_pool() -> &'static rayon::ThreadPool {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let threads = (cores / 2).max(2);
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("analysis-{}", i))
            .build()
            .expect("failed to build analysis thread pool")
    })
}

fn analyse_single(entry: &FileEntry) -> Option<FileFragInfo> {
    let h = open_file_for_query(&entry.path).ok()?;
    let runs = winapi::get_retrieval_pointers(h).unwrap_or_default();
    close_handle(h);
    if runs.is_empty() { return None; }
    let fc = runs.len() as u32;
    Some(FileFragInfo {
        path: entry.path.clone(), size_bytes: entry.size_bytes,
        fragment_count: fc, runs, is_fragmented: fc > 1,
    })
}

#[cfg(target_os = "windows")]
fn open_file_for_query(path: &std::path::Path) -> crate::defrag_engine::errors::DefragResult<isize> {
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE},
            Storage::FileSystem::{CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
        },
    };
    let wide: Vec<u16> = path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let h = unsafe {
        CreateFileW(PCWSTR(wide.as_ptr()), GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, None)
    }.map_err(|e| anyhow::anyhow!("{}", e))?;
    if h == INVALID_HANDLE_VALUE { anyhow::bail!("INVALID_HANDLE_VALUE"); }
    Ok(h.0 as isize)
}

#[cfg(not(target_os = "windows"))]
fn open_file_for_query(_: &std::path::Path) -> crate::defrag_engine::errors::DefragResult<isize> { Ok(1) }

#[cfg(target_os = "windows")]
fn close_handle(h: isize) {
    unsafe { let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(h as _)); }
}
#[cfg(not(target_os = "windows"))]
fn close_handle(_: isize) {}
