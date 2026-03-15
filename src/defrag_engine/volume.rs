use crate::defrag_engine::{errors::{DefragError, DefragResult}, winapi::{self, VolumeHandle}};
use log::debug;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub device_path:    String,
    pub label:          String,
    pub filesystem:     String,
    pub cluster_size:   u64,
    pub total_clusters: i64,
    pub free_clusters:  i64,
    pub total_bytes:    u64,
    pub free_bytes:     u64,
}

impl VolumeInfo {
    pub fn used_pct(&self) -> f64 {
        if self.total_clusters == 0 { return 0.0; }
        (self.total_clusters - self.free_clusters) as f64 / self.total_clusters as f64 * 100.0
    }
}

#[derive(Debug)]
pub struct VolumeBitmap {
    pub starting_lcn:   i64,
    pub total_clusters: i64,
    pub bytes:          Vec<u8>,
}

impl VolumeBitmap {
    pub fn is_used(&self, lcn: i64) -> bool {
        let offset = lcn - self.starting_lcn;
        if offset < 0 || offset >= self.total_clusters { return true; }
        let bi = (offset / 8) as usize;
        let bb = (offset % 8) as u8;
        if bi >= self.bytes.len() { return false; }
        (self.bytes[bi] >> bb) & 1 == 1
    }
    pub fn is_free(&self, lcn: i64) -> bool { !self.is_used(lcn) }
    pub fn free_count(&self) -> i64 {
        let raw: i64 = self.bytes.iter().map(|b| b.count_zeros() as i64).sum();
        raw.min(self.total_clusters)
    }
    pub fn find_free_run(&self, length: i64, hint: i64) -> Option<i64> {
        let start = (hint - self.starting_lcn).max(0);
        let end   = self.total_clusters;
        let mut rs = -1i64;
        let mut rl = 0i64;
        let mut c  = start;
        while c < end {
            if self.is_free(c + self.starting_lcn) {
                if rs < 0 { rs = c; }
                rl += 1;
                if rl >= length { return Some(rs + self.starting_lcn); }
            } else { rs = -1; rl = 0; }
            c += 1;
        }
        None
    }
    /// Build a compact 100×1 row of states for cluster-map display.
    pub fn compact_row(&self, cols: usize) -> Vec<u8> {
        let step = (self.total_clusters as usize).max(1) / cols.max(1);
        (0..cols).map(|i| {
            let lcn = self.starting_lcn + (i * step) as i64;
            if self.is_used(lcn) { 2 } else { 0 }
        }).collect()
    }
}

pub fn open_volume(device_path: &str, drive_label: &str) -> DefragResult<(VolumeHandle, VolumeInfo)> {
    let filesystem = winapi::get_filesystem_type(drive_label)?;
    if !filesystem.eq_ignore_ascii_case("NTFS") {
        anyhow::bail!(DefragError::UnsupportedFilesystem(filesystem));
    }
    let handle = winapi::open_volume(device_path)?;
    let cluster_size = get_cluster_size(drive_label)?;
    let (starting_lcn, total_clusters, bitmap_bytes) = winapi::get_volume_bitmap(&handle)?;
    let bm = VolumeBitmap { starting_lcn, total_clusters, bytes: bitmap_bytes };
    let free_clusters = bm.free_count();
    let info = VolumeInfo {
        device_path: device_path.to_string(),
        label: drive_label.to_string(),
        filesystem,
        cluster_size,
        total_clusters,
        free_clusters,
        total_bytes: total_clusters as u64 * cluster_size,
        free_bytes:  free_clusters  as u64 * cluster_size,
    };
    Ok((handle, info))
}

pub fn load_volume_info_basic(device_path: &str, drive_label: &str) -> DefragResult<VolumeInfo> {
    let filesystem = winapi::get_filesystem_type(drive_label)?;
    if !filesystem.eq_ignore_ascii_case("NTFS") {
        anyhow::bail!(DefragError::UnsupportedFilesystem(filesystem));
    }
    let (cluster_size, total_clusters, free_clusters) = query_disk_geometry(drive_label)?;
    Ok(VolumeInfo {
        device_path: device_path.to_string(),
        label: drive_label.to_string(),
        filesystem,
        cluster_size,
        total_clusters,
        free_clusters,
        total_bytes: total_clusters as u64 * cluster_size,
        free_bytes: free_clusters as u64 * cluster_size,
    })
}

pub fn load_bitmap(handle: &VolumeHandle) -> DefragResult<VolumeBitmap> {
    let (starting_lcn, total_clusters, bytes) = winapi::get_volume_bitmap(handle)?;
    Ok(VolumeBitmap { starting_lcn, total_clusters, bytes })
}

pub fn get_cluster_size(drive_label: &str) -> DefragResult<u64> {
    let (cluster_size, _, _) = query_disk_geometry(drive_label)?;
    Ok(cluster_size)
}

fn query_disk_geometry(drive_label: &str) -> DefragResult<(u64, i64, i64)> {
    #[cfg(target_os = "windows")]
    {
        use windows::{core::PCWSTR, Win32::Storage::FileSystem::GetDiskFreeSpaceW};
        let root = format!("{}\\", drive_label);
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let mut spc: u32 = 0; let mut bps: u32 = 0; let mut fc: u32 = 0; let mut tc: u32 = 0;
        unsafe {
            GetDiskFreeSpaceW(PCWSTR(wide.as_ptr()), Some(&mut spc), Some(&mut bps), Some(&mut fc), Some(&mut tc))
        }.map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            crate::defrag_engine::errors::DefragError::ApiFailure { api: "GetDiskFreeSpaceW", code }
        })?;
        Ok((spc as u64 * bps as u64, tc as i64, fc as i64))
    }
    #[cfg(not(target_os = "windows"))]
    { Ok((4096, 0, 0)) }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path:       PathBuf,
    pub size_bytes: u64,
    pub is_system:  bool,
    pub is_temp:    bool,
}

pub fn enumerate_files(root_dir: &Path) -> DefragResult<Vec<FileEntry>> {
    let mut entries = Vec::new();
    enumerate_recursive(root_dir, &mut entries);
    Ok(entries)
}

fn enumerate_recursive(dir: &Path, out: &mut Vec<FileEntry>) {
    let rd = match std::fs::read_dir(dir) { Ok(r) => r, Err(_) => return };
    for entry in rd.flatten() {
        let path = entry.path();
        let meta = match entry.metadata() { Ok(m) => m, Err(_) => continue };
        if meta.is_dir() { enumerate_recursive(&path, out); }
        else if meta.is_file() {
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
            let is_system = fname.starts_with('$')
                || matches!(fname.as_str(), "pagefile.sys" | "hiberfil.sys" | "swapfile.sys");
            #[cfg(target_os = "windows")]
            let is_system = { use std::os::windows::fs::MetadataExt;
                is_system || (meta.file_attributes() & 0x0004 != 0) };
            #[cfg(target_os = "windows")]
            let is_temp = { use std::os::windows::fs::MetadataExt;
                meta.file_attributes() & 0x0100 != 0 };
            #[cfg(not(target_os = "windows"))]
            let is_temp = false;
            out.push(FileEntry { path, size_bytes: meta.len(), is_system, is_temp });
        }
    }
}
