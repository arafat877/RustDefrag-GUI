/// winapi.rs — Safe wrappers around Windows filesystem control APIs.
/// All unsafe code lives here and nowhere else.

use crate::defrag_engine::errors::{DefragError, DefragResult};
use log::debug;

pub const FSCTL_GET_VOLUME_BITMAP: u32      = 0x0009_006F;
pub const FSCTL_GET_RETRIEVAL_POINTERS: u32 = 0x0009_0073;
pub const FSCTL_MOVE_FILE: u32              = 0x0009_0074;

// ── C-compatible structures ──────────────────────────────────────────────────

#[repr(C)]
pub struct StartingLcnInputBuffer { pub starting_lcn: i64 }

#[repr(C)]
pub struct VolumeBitmapBuffer {
    pub starting_lcn: i64,
    pub bitmap_size:  i64,
    pub buffer: [u8; 1],
}

#[repr(C)]
pub struct StartingVcnInputBuffer { pub starting_vcn: i64 }

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RetrievalPointerExtent { pub next_vcn: i64, pub lcn: i64 }

#[repr(C)]
pub struct RetrievalPointersBuffer {
    pub extent_count: u32,
    pub _padding:     u32,
    pub starting_vcn: i64,
    pub extents: [RetrievalPointerExtent; 1],
}

#[repr(C)]
pub struct MoveFileData {
    pub file_handle:   isize,
    pub starting_vcn:  i64,
    pub starting_lcn:  i64,
    pub cluster_count: u32,
}

// ── Privilege check ─────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn is_elevated() -> bool {
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len: u32 = 0;
        let ok = GetTokenInformation(
            token, TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        let _ = CloseHandle(token);
        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_elevated() -> bool { true }

// ── VolumeHandle ────────────────────────────────────────────────────────────

pub struct VolumeHandle(
    #[cfg(target_os = "windows")] windows::Win32::Foundation::HANDLE,
    #[cfg(not(target_os = "windows"))] i64,
);

impl VolumeHandle {
    #[cfg(target_os = "windows")]
    pub fn raw(&self) -> windows::Win32::Foundation::HANDLE { self.0 }
    #[cfg(not(target_os = "windows"))]
    pub fn raw(&self) -> i64 { self.0 }
}

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        unsafe { let _ = windows::Win32::Foundation::CloseHandle(self.0); }
    }
}

// ── open_volume ──────────────────────────────────────────────────────────────

pub fn open_volume(path: &str) -> DefragResult<VolumeHandle> {
    debug!("Opening volume: {}", path);
    #[cfg(target_os = "windows")]
    {
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
                Storage::FileSystem::{CreateFileW, FILE_FLAG_NO_BUFFERING, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
            },
        };
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let h = unsafe {
            CreateFileW(PCWSTR(wide.as_ptr()), (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAG_NO_BUFFERING, None)
        }.map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure { api: "CreateFileW", code }
        })?;
        if h == INVALID_HANDLE_VALUE { anyhow::bail!(DefragError::InvalidVolume(path.to_string())); }
        Ok(VolumeHandle(h))
    }
    #[cfg(not(target_os = "windows"))]
    { Ok(VolumeHandle(0)) }
}

pub fn open_volume_readonly(path: &str) -> DefragResult<VolumeHandle> {
    debug!("Opening volume (read-only): {}", path);
    #[cfg(target_os = "windows")]
    {
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE},
                Storage::FileSystem::{CreateFileW, FILE_FLAG_NO_BUFFERING, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
            },
        };
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let h = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_NO_BUFFERING,
                None,
            )
        }.map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure { api: "CreateFileW", code }
        })?;
        if h == INVALID_HANDLE_VALUE {
            anyhow::bail!(DefragError::InvalidVolume(path.to_string()));
        }
        Ok(VolumeHandle(h))
    }
    #[cfg(not(target_os = "windows"))]
    { Ok(VolumeHandle(0)) }
}

// ── get_filesystem_type ──────────────────────────────────────────────────────

pub fn get_filesystem_type(drive_label: &str) -> DefragResult<String> {
    #[cfg(target_os = "windows")]
    {
        use windows::{core::PCWSTR, Win32::Storage::FileSystem::GetVolumeInformationW};
        let root = format!("{}\\", drive_label);
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let mut fs_name = vec![0u16; 64];
        unsafe {
            GetVolumeInformationW(PCWSTR(wide.as_ptr()), None, None, None, None, Some(fs_name.as_mut_slice()))
        }.map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure { api: "GetVolumeInformationW", code }
        })?;
        let len = fs_name.iter().position(|&c| c == 0).unwrap_or(fs_name.len());
        Ok(String::from_utf16_lossy(&fs_name[..len]))
    }
    #[cfg(not(target_os = "windows"))]
    { Ok("NTFS".to_string()) }
}

// ── get_volume_bitmap ────────────────────────────────────────────────────────

pub fn get_volume_bitmap(vol: &VolumeHandle) -> DefragResult<(i64, i64, Vec<u8>)> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::IO::DeviceIoControl;
        let buf_size: usize = 1 << 22;
        let mut output: Vec<u8> = vec![0u8; buf_size];
        let input = StartingLcnInputBuffer { starting_lcn: 0 };
        let mut bytes_returned: u32 = 0;
        unsafe {
            DeviceIoControl(vol.raw(), FSCTL_GET_VOLUME_BITMAP,
                Some(&input as *const _ as *const _),
                std::mem::size_of::<StartingLcnInputBuffer>() as u32,
                Some(output.as_mut_ptr() as *mut _), buf_size as u32,
                Some(&mut bytes_returned), None)
        }.map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure { api: "FSCTL_GET_VOLUME_BITMAP", code }
        })?;
        let hdr = unsafe { &*(output.as_ptr() as *const VolumeBitmapBuffer) };
        let bitmap_bytes = (hdr.bitmap_size as usize + 7) / 8;
        let bitmap = output[16..16 + bitmap_bytes].to_vec();
        Ok((hdr.starting_lcn, hdr.bitmap_size, bitmap))
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Synthetic realistic bitmap for testing
        let total: i64 = 1_000_000;
        let bytes = (total as usize + 7) / 8;
        let mut bm = vec![0u8; bytes];
        let mut rng = 12345u64;
        for i in 0..bytes {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            bm[i] = if (i as f64 / bytes as f64) < 0.87 { (rng >> 32) as u8 } else { 0 };
        }
        Ok((0, total, bm))
    }
}

// ── get_retrieval_pointers ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ClusterRun { pub vcn: i64, pub lcn: i64, pub length: i64 }

pub fn get_retrieval_pointers(file_handle: isize) -> DefragResult<Vec<ClusterRun>> {
    #[cfg(target_os = "windows")]
    {
        use windows::{
            Win32::Foundation::HANDLE,
            Win32::System::IO::DeviceIoControl,
        };
        let handle = HANDLE(file_handle as _);
        let mut runs = Vec::new();
        let mut starting_vcn: i64 = 0;
        let buf_size = 1usize << 16;
        let mut output = vec![0u8; buf_size];
        loop {
            let input = StartingVcnInputBuffer { starting_vcn };
            let mut bytes_returned: u32 = 0;
            let result = unsafe {
                DeviceIoControl(handle, FSCTL_GET_RETRIEVAL_POINTERS,
                    Some(&input as *const _ as *const _),
                    std::mem::size_of::<StartingVcnInputBuffer>() as u32,
                    Some(output.as_mut_ptr() as *mut _), buf_size as u32,
                    Some(&mut bytes_returned), None)
            };
            let more = match &result {
                Err(_) => {
                    let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
                    if code == 234 { true } else { break; }
                }
                Ok(_) => false,
            };
            let hdr = unsafe { &*(output.as_ptr() as *const RetrievalPointersBuffer) };
            let extents: &[RetrievalPointerExtent] = unsafe {
                std::slice::from_raw_parts(&hdr.extents as *const _, hdr.extent_count as usize)
            };
            let mut vcn = hdr.starting_vcn;
            for ext in extents {
                let length = ext.next_vcn - vcn;
                if ext.lcn != -1 { runs.push(ClusterRun { vcn, lcn: ext.lcn, length }); }
                vcn = ext.next_vcn;
            }
            if !more { break; }
            if let Some(last) = extents.last() { starting_vcn = last.next_vcn; } else { break; }
        }
        Ok(runs)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut rng = file_handle as u64;
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        let frag_count = (rng % 5) as usize + 1;
        let mut runs = Vec::new();
        let mut vcn = 0i64;
        for _ in 0..frag_count {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let len = (rng % 20 + 1) as i64;
            let lcn = (rng % 900_000 + 100) as i64;
            runs.push(ClusterRun { vcn, lcn, length: len });
            vcn += len;
        }
        Ok(runs)
    }
}

// ── move_file_clusters ───────────────────────────────────────────────────────

pub fn move_file_clusters(
    vol: &VolumeHandle, file_handle: isize,
    starting_vcn: i64, starting_lcn: i64, cluster_count: u32,
) -> DefragResult<()> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::IO::DeviceIoControl;
        let input = MoveFileData { file_handle, starting_vcn, starting_lcn, cluster_count };
        let mut bytes_returned: u32 = 0;
        unsafe {
            DeviceIoControl(vol.raw(), FSCTL_MOVE_FILE,
                Some(&input as *const _ as *const _),
                std::mem::size_of::<MoveFileData>() as u32,
                None, 0, Some(&mut bytes_returned), None)
        }.map_err(|_| {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            DefragError::ApiFailure { api: "FSCTL_MOVE_FILE", code }
        })?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        debug!("move stub: vcn={} lcn={} count={}", starting_vcn, starting_lcn, cluster_count);
        Ok(())
    }
}

// ── Boot-time defrag queue ───────────────────────────────────────────────────

/// Queue a file for defragmentation at next Windows boot via MoveFileEx.
#[cfg(target_os = "windows")]
pub fn queue_boot_move(src: &str, dst: Option<&str>) -> DefragResult<()> {
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT},
    };
    let src_w: Vec<u16> = src.encode_utf16().chain(std::iter::once(0)).collect();
    let dst_w: Option<Vec<u16>> = dst.map(|d| d.encode_utf16().chain(std::iter::once(0)).collect());
    let dst_ptr = dst_w
        .as_ref()
        .map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr()));
    unsafe {
        MoveFileExW(
            PCWSTR(src_w.as_ptr()),
            dst_ptr,
            MOVEFILE_DELAY_UNTIL_REBOOT,
        )
    }.map_err(|e| anyhow::anyhow!("MoveFileExW boot queue: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn queue_boot_move(_src: &str, _dst: Option<&str>) -> DefragResult<()> { Ok(()) }

// ── Set high priority ────────────────────────────────────────────────────────

pub fn set_high_priority() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::System::Threading::{GetCurrentProcess, SetPriorityClass, HIGH_PRIORITY_CLASS};
        let _ = SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS);
    }
}
