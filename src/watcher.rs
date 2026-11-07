use std::collections::HashMap;
use std::ffi::CString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::ChildStdin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::spawn_compact_thread;
use crate::theme::{CYAN, RESET, SUB, print_tagged_event};

const IN_MODIFY: u32 = 0x0000_0002;
const IN_CLOSE_WRITE: u32 = 0x0000_0008;
const IN_MOVED_TO: u32 = 0x0000_0080;
const IN_CREATE: u32 = 0x0000_0100;
const IN_ISDIR: u32 = 0x4000_0000;
const WATCH_MASK: u32 = IN_MODIFY | IN_CLOSE_WRITE | IN_MOVED_TO | IN_CREATE;
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(250);

unsafe extern "C" {
    fn inotify_init1(flags: i32) -> i32;
    fn inotify_add_watch(fd: i32, pathname: *const i8, mask: u32) -> i32;
}

#[repr(C)]
struct InotifyEventHeader {
    wd: i32,
    mask: u32,
    cookie: u32,
    len: u32,
}

fn watch_dir_recursive(fd: i32, dir: &Path, wd_map: &mut HashMap<i32, PathBuf>) {
    let Ok(c_path) = CString::new(dir.as_os_str().as_bytes()) else {
        return;
    };
    let wd = unsafe { inotify_add_watch(fd, c_path.as_ptr(), WATCH_MASK) };
    if wd >= 0 {
        wd_map.insert(wd, dir.to_path_buf());
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if !name.as_bytes().starts_with(b".") {
                watch_dir_recursive(fd, &path, wd_map);
            }
        }
    }
}

/// Spawns a zero-polling Linux `inotify` watcher on `lib/` that triggers Hot Reload (`r\n`)
/// whenever a `.dart` file is saved.
pub fn spawn_dart_watcher(enabled: Arc<AtomicBool>, stdin_slot: Arc<Mutex<Option<ChildStdin>>>) {
    spawn_compact_thread("ff-watch", move || {
        let lib_dir = PathBuf::from("lib");
        if !lib_dir.is_dir() {
            return;
        }

        let fd = unsafe { inotify_init1(0) };
        if fd < 0 {
            return;
        }

        let mut inotify_file = unsafe { File::from_raw_fd(fd) };
        let mut wd_map: HashMap<i32, PathBuf> = HashMap::new();
        watch_dir_recursive(fd, &lib_dir, &mut wd_map);

        let mut buf = [0u8; 4096];
        let mut last_reload = Instant::now()
            .checked_sub(DEBOUNCE_WINDOW)
            .unwrap_or_else(Instant::now);

        while let Ok(n) = inotify_file.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut offset = 0usize;
            let mut changed_dart: Option<String> = None;

            while offset + std::mem::size_of::<InotifyEventHeader>() <= n {
                let hdr = unsafe {
                    std::ptr::read_unaligned(buf.as_ptr().add(offset) as *const InotifyEventHeader)
                };
                let name_start = offset + std::mem::size_of::<InotifyEventHeader>();
                let name_end = (name_start + hdr.len as usize).min(n);
                let name_bytes = &buf[name_start..name_end];
                let nul_pos = name_bytes
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(name_bytes.len());
                let file_name = std::str::from_utf8(&name_bytes[..nul_pos]).unwrap_or("");

                if (hdr.mask & IN_ISDIR) != 0
                    && (hdr.mask & IN_CREATE) != 0
                    && !file_name.starts_with('.')
                    && let Some(parent) = wd_map.get(&hdr.wd).cloned()
                {
                    watch_dir_recursive(fd, &parent.join(file_name), &mut wd_map);
                } else if file_name.ends_with(".dart") && !file_name.starts_with('.') {
                    let rel = wd_map
                        .get(&hdr.wd)
                        .map(|p| p.join(file_name).display().to_string())
                        .unwrap_or_else(|| file_name.to_string());
                    changed_dart = Some(rel);
                }

                offset = name_end;
            }

            if let Some(dart_path) = changed_dart
                && enabled.load(Ordering::Relaxed)
                && last_reload.elapsed() >= DEBOUNCE_WINDOW
            {
                last_reload = Instant::now();
                if let Some(stdin) = &mut *stdin_slot.lock().unwrap() {
                    let _ = stdin.write_all(b"r\n");
                    let _ = stdin.flush();
                    print_tagged_event(
                        "HOT",
                        CYAN,
                        &format!("auto-reload {}({}){}", SUB, dart_path, RESET),
                    );
                }
            }
        }
    });
}
