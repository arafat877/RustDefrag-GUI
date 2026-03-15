/// worker.rs — Background engine thread: receives commands, runs defrag operations,
/// sends progress events back to the GUI via mpsc channel.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc};
use log::{error, info, warn};

use crate::engine::messages::{EngineCommand, EngineEvent};
use crate::defrag_engine::{
    analyzer,
    defrag::{self, DefragOptions},
    volume::{self, VolumeBitmap},
    winapi,
};

const MAP_COLS: usize = 100;
const MAP_ROWS: usize = 25;

pub fn spawn_worker(
    cmd_rx:   mpsc::Receiver<EngineCommand>,
    evt_tx:   mpsc::SyncSender<EngineEvent>,
    stop_flag: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name("defrag-engine".to_string())
        .spawn(move || {
            run_loop(cmd_rx, evt_tx, stop_flag);
        })
        .expect("Failed to spawn engine thread");
}

fn run_loop(
    cmd_rx:   mpsc::Receiver<EngineCommand>,
    evt_tx:   mpsc::SyncSender<EngineEvent>,
    stop_flag: Arc<AtomicBool>,
) {
    while let Ok(cmd) = cmd_rx.recv() {
        stop_flag.store(false, Ordering::SeqCst);
        match cmd {
            EngineCommand::Stop => {
                stop_flag.store(true, Ordering::SeqCst);
                let _ = evt_tx.send(EngineEvent::Stopped);
            }
            EngineCommand::StartAnalysis { drive } => {
                run_analysis(&drive, &evt_tx, &stop_flag);
            }
            EngineCommand::StartDefrag { drive, compact_mode, boot_fallback } => {
                run_defrag(&drive, compact_mode, boot_fallback, &evt_tx, &stop_flag);
            }
        }
    }
}

fn send(tx: &mpsc::SyncSender<EngineEvent>, ev: EngineEvent) {
    if tx.try_send(ev).is_err() {
        warn!("Event channel full — dropping event");
    }
}

fn run_analysis(
    drive:     &str,
    tx:        &mpsc::SyncSender<EngineEvent>,
    stop_flag: &Arc<AtomicBool>,
) {
    info!("Analysis starting on {}", drive);
    let device_path  = format!("\\\\.\\{}", drive);
    let drive_label  = drive.trim_end_matches('\\').to_uppercase();

    // Load basic volume info without requiring raw write access.
    let vol_info = match volume::load_volume_info_basic(&device_path, &drive_label) {
        Ok(info) => info,
        Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
    };
    send(tx, EngineEvent::VolumeReady(Box::new(vol_info.clone())));
    // Immediate first paint so UI doesn't look idle before bitmap retrieval starts.
    send(tx, EngineEvent::BitmapReady(estimated_map_from_usage(vol_info.used_pct())));

    // Best effort: try opening the raw volume read-only for bitmap visualization.
    // If this fails (e.g. not elevated), continue analysis without the cluster map.
    let vol_handle = match winapi::open_volume_readonly(&device_path) {
        Ok(r) => r,
        Err(_) => {
            info!("Raw volume open unavailable for analysis on {}", drive_label);
            send(tx, EngineEvent::BitmapReady(estimated_map_from_usage(vol_info.used_pct())));
            // Enumerate/analyse can still proceed.
            let root = std::path::PathBuf::from(format!("{}\\", drive_label));
            let files = match volume::enumerate_files(&root) {
                Ok(f) => f,
                Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
            };
            let total_files = files.len();
            send(tx, EngineEvent::EnumComplete { total_files });

            let tx2 = tx.clone();
            let sf2  = stop_flag.clone();
            let total_clusters = vol_info.total_clusters;
            let report = match analyzer::analyse_files(&files, sf2.clone(), move |tick| {
                send(&tx2, EngineEvent::AnalysisProgress {
                    done: tick.done,
                    total: tick.total,
                    frag_so_far: tick.frag_so_far,
                    cluster_events: runs_to_cluster_events(&tick.new_fragmented_runs, total_clusters),
                });
            }) {
                Ok(r) => r,
                Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
            };

            if stop_flag.load(Ordering::Relaxed) {
                send(tx, EngineEvent::Stopped);
                return;
            }
            let mut final_map = estimated_map_from_usage(vol_info.used_pct());
            overlay_fragmented_runs_on_map(&mut final_map, &report.fragmented, vol_info.total_clusters);
            send(tx, EngineEvent::BitmapReady(final_map));
            send(tx, EngineEvent::AnalysisComplete(Box::new(report)));
            return;
        }
    };

    // Load bitmap for cluster map display
    let mut analysis_bitmap: Option<VolumeBitmap> = None;
    if let Ok(bm) = volume::load_bitmap(&vol_handle) {
        send(tx, EngineEvent::BitmapReady(bitmap_to_map(&bm, &drive_label)));
        analysis_bitmap = Some(bm);
    }

    // Enumerate files
    let root = std::path::PathBuf::from(format!("{}\\", drive_label));
    let files = match volume::enumerate_files(&root) {
        Ok(f) => f,
        Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
    };
    let total_files = files.len();
    send(tx, EngineEvent::EnumComplete { total_files });

    // Analyse
    let tx2 = tx.clone();
    let sf2  = stop_flag.clone();
    let total_clusters = vol_info.total_clusters;
    let report = match analyzer::analyse_files(&files, sf2.clone(), move |tick| {
        send(&tx2, EngineEvent::AnalysisProgress {
            done: tick.done,
            total: tick.total,
            frag_so_far: tick.frag_so_far,
            cluster_events: runs_to_cluster_events(&tick.new_fragmented_runs, total_clusters),
        });
    }) {
        Ok(r) => r,
        Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
    };

    if stop_flag.load(Ordering::Relaxed) {
        send(tx, EngineEvent::Stopped);
        return;
    }

    let mut final_map = if let Some(bm) = &analysis_bitmap {
        bitmap_to_map(bm, &drive_label)
    } else {
        estimated_map_from_usage(vol_info.used_pct())
    };
    overlay_fragmented_runs_on_map(&mut final_map, &report.fragmented, vol_info.total_clusters);
    send(tx, EngineEvent::BitmapReady(final_map));
    send(tx, EngineEvent::AnalysisComplete(Box::new(report)));
}

fn run_defrag(
    drive:        &str,
    compact_mode: bool,
    boot_fallback:bool,
    tx:           &mpsc::SyncSender<EngineEvent>,
    stop_flag:    &Arc<AtomicBool>,
) {
    info!("Defrag starting on {} (compact={}, boot_fb={})", drive, compact_mode, boot_fallback);
    let device_path = format!("\\\\.\\{}", drive);
    let drive_label = drive.trim_end_matches('\\').to_uppercase();

    let (vol_handle, vol_info) = match volume::open_volume(&device_path, &drive_label) {
        Ok(r) => r,
        Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
    };
    send(tx, EngineEvent::VolumeReady(Box::new(vol_info.clone())));

    // Load bitmap
    let bm_raw = match volume::load_bitmap(&vol_handle) {
        Ok(b) => b,
        Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
    };
    send(tx, EngineEvent::BitmapReady(bitmap_to_map(&bm_raw, &drive_label)));

    // Enumerate + analyse (we need the fragmented file list)
    let root  = std::path::PathBuf::from(format!("{}\\", drive_label));
    let files = match volume::enumerate_files(&root) {
        Ok(f) => f,
        Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
    };
    send(tx, EngineEvent::EnumComplete { total_files: files.len() });

    let tx2  = tx.clone();
    let sf2  = stop_flag.clone();
    let total_clusters = vol_info.total_clusters;
    let report = match analyzer::analyse_files(&files, sf2, move |tick| {
        send(&tx2, EngineEvent::AnalysisProgress {
            done: tick.done,
            total: tick.total,
            frag_so_far: tick.frag_so_far,
            cluster_events: runs_to_cluster_events(&tick.new_fragmented_runs, total_clusters),
        });
    }) {
        Ok(r) => r,
        Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
    };

    if stop_flag.load(Ordering::Relaxed) { send(tx, EngineEvent::Stopped); return; }

    let targets      = report.fragmented.clone();
    let mut bitmap   = bm_raw;
    let cluster_size = vol_info.cluster_size;

    let opts = DefragOptions { compact_mode, boot_fallback, cluster_size };

    let tx3      = tx.clone();
    let sf3      = stop_flag.clone();

    let stats = match defrag::defragment(&vol_handle, &targets, &mut bitmap, &opts, sf3, move |upd| {
        send(&tx3, EngineEvent::DefragProgress {
            file_index:      upd.file_index,
            total_files:     upd.total_files,
            current_file:    upd.current_file,
            clusters_moved:  upd.clusters_moved,
            files_defragged: upd.files_defragged,
            files_skipped:   upd.files_skipped,
            files_in_use:    upd.files_in_use,
            bytes_moved:     upd.clusters_moved * cluster_size,
            cluster_events:  upd.cluster_events,
        });
    }) {
        Ok(s) => s,
        Err(e) => { send(tx, EngineEvent::Error(e.to_string())); return; }
    };

    if stop_flag.load(Ordering::Relaxed) { send(tx, EngineEvent::Stopped); return; }
    send(tx, EngineEvent::DefragComplete(Box::new(stats)));
}

// ── Bitmap → cluster map conversion ─────────────────────────────────────────

fn bitmap_to_map(bm: &VolumeBitmap, _label: &str) -> Vec<Vec<u8>> {
    let total = bm.total_clusters.max(1) as usize;
    let cells_per_cell = (total + MAP_COLS * MAP_ROWS - 1) / (MAP_COLS * MAP_ROWS);

    let mut map = vec![vec![0u8; MAP_COLS]; MAP_ROWS];
    for row in 0..MAP_ROWS {
        for col in 0..MAP_COLS {
            let cell_idx = row * MAP_COLS + col;
            let lcn_start = bm.starting_lcn + (cell_idx * cells_per_cell) as i64;
            let lcn_end = (lcn_start + cells_per_cell as i64)
                .min(bm.starting_lcn + bm.total_clusters);
            if lcn_end <= lcn_start {
                map[row][col] = 0;
                continue;
            }

            let mut used = 0usize;
            let mut prev = bm.is_used(lcn_start);
            if prev { used += 1; }
            let mut transitions = 0usize;
            let mut lcn = lcn_start + 1;
            while lcn < lcn_end {
                let cur = bm.is_used(lcn);
                if cur { used += 1; }
                if cur != prev { transitions += 1; }
                prev = cur;
                lcn += 1;
            }

            let span = (lcn_end - lcn_start) as usize;
            let ratio = used as f32 / span.max(1) as f32;
            let in_metadata_zone = lcn_start < bm.starting_lcn + (bm.total_clusters / 64).max(8192);

            // 0=free, 2=used, 3=fragmented (mixed/high-transition occupancy).
            map[row][col] = if used == 0 {
                0
            } else if in_metadata_zone && ratio > 0.35 && transitions <= 3 {
                1
            } else if used == span {
                if transitions >= 3 { 3 } else { 2 }
            } else if transitions >= 2 || (ratio > 0.15 && ratio < 0.85) {
                3
            } else if ratio >= 0.5 {
                2
            } else {
                0
            };
        }
    }
    // Ensure metadata/system area is visible near the start of disk.
    let sys_cells = (MAP_COLS * MAP_ROWS / 120).max(16);
    for idx in 0..sys_cells {
        let r = idx / MAP_COLS;
        let c = idx % MAP_COLS;
        if r < MAP_ROWS && map[r][c] != 0 {
            map[r][c] = 1;
        }
    }
    map
}

fn estimated_map_from_usage(used_pct: f64) -> Vec<Vec<u8>> {
    let mut map = vec![vec![0u8; MAP_COLS]; MAP_ROWS];
    let total_cells = MAP_COLS * MAP_ROWS;
    let used_cells = ((used_pct.clamp(0.0, 100.0) / 100.0) * total_cells as f64) as usize;

    for row in 0..MAP_ROWS {
        for col in 0..MAP_COLS {
            let idx = row * MAP_COLS + col;
            if idx < used_cells {
                // Sprinkle some fragmented cells to make activity visible.
                map[row][col] = if idx % 17 == 0 { 3 } else { 2 };
                if idx < (MAP_COLS * MAP_ROWS / 120).max(16) {
                    map[row][col] = 1;
                }
            } else {
                map[row][col] = 0;
            }
        }
    }
    map
}

fn overlay_fragmented_runs_on_map(
    map: &mut [Vec<u8>],
    files: &[crate::defrag_engine::analyzer::FileFragInfo],
    total_clusters: i64,
) {
    if total_clusters <= 0 {
        return;
    }
    for fi in files {
        for run in &fi.runs {
            if run.length <= 0 {
                continue;
            }
            // Paint deterministic sample points across each run.
            let steps = run.length.min(8);
            for i in 0..steps {
                let lcn = run.lcn + (run.length * i) / steps.max(1);
                let frac = (lcn as f64 / total_clusters as f64).clamp(0.0, 0.9999);
                let cell_idx = (frac * (MAP_COLS * MAP_ROWS) as f64) as usize;
                let row = cell_idx / MAP_COLS;
                let col = cell_idx % MAP_COLS;
                if row < MAP_ROWS && col < MAP_COLS {
                    map[row][col] = 3;
                }
            }
        }
    }
}

fn runs_to_cluster_events(runs: &[crate::defrag_engine::winapi::ClusterRun], total_clusters: i64) -> Vec<(i64, u8)> {
    if total_clusters <= 0 || runs.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for run in runs {
        if run.length <= 0 {
            continue;
        }
        // Mark both run boundaries and a few interior points so long runs paint the map.
        out.push((run.lcn, 4));
        out.push((run.lcn + run.length - 1, 4));
        let steps = run.length.min(8);
        for i in 1..steps {
            let lcn = run.lcn + (run.length * i) / steps;
            out.push((lcn, 4));
        }
    }
    out
}
