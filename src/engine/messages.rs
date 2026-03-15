use crate::defrag_engine::{analyzer::FragmentationReport, defrag::DefragStats, volume::VolumeInfo};

/// Messages sent FROM the engine worker TO the GUI.
#[derive(Debug)]
pub enum EngineEvent {
    // Enumeration
    EnumProgress { done: usize, total: usize },
    EnumComplete { total_files: usize },

    // Analysis
    AnalysisProgress {
        done: usize,
        total: usize,
        frag_so_far: u64,
        /// (lcn, new_state) pairs for cluster map updates during analysis
        cluster_events: Vec<(i64, u8)>,
    },
    AnalysisComplete(Box<FragmentationReport>),

    // Defrag
    DefragProgress {
        file_index:      usize,
        total_files:     usize,
        current_file:    String,
        clusters_moved:  u64,
        files_defragged: u64,
        files_skipped:   u64,
        files_in_use:    u64,
        bytes_moved:     u64,
        /// (lcn, new_state) pairs for cluster map update
        cluster_events:  Vec<(i64, u8)>,
    },
    DefragComplete(Box<DefragStats>),

    // Volume info loaded
    VolumeReady(Box<VolumeInfo>),

    // Bitmap loaded / updated (compact representation, 100 cols × 25 rows)
    BitmapReady(Vec<Vec<u8>>),   // [row][col] = state

    Error(String),
    Stopped,
}

/// Messages sent FROM the GUI TO the engine worker.
#[derive(Debug)]
pub enum EngineCommand {
    StartAnalysis { drive: String },
    StartDefrag   { drive: String, compact_mode: bool, boot_fallback: bool },
    Stop,
}
