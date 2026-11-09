//! Starting the game with DLLs loaded, without Mod Engine 2: the game is created suspended, runs
//! up to its entry point (its entry is made a jump to itself, so the process is set up, its own
//! DLLs loaded and its TLS callbacks run, as when Mod Engine 2 loads DLLs), each DLL is loaded
//! into it in order (a remote `LoadLibraryW`), the entry is put back and the game runs. Loading
//! before the process is set up (the first way) worked on one PC and hung on another. Steam
//! needs the app id in the environment when the game isn't started by Steam itself.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::Debug::{
    CONTEXT, CONTEXT_CONTROL_AMD64, CONTEXT_INTEGER_AMD64, FlushInstructionCache, GetThreadContext, ReadProcessMemory, WriteProcessMemory,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx, VirtualProtectEx,
};
use windows::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, CreateRemoteThread, GetExitCodeThread, PROCESS_INFORMATION, ResumeThread, STARTUPINFOW,
    SuspendThread, TerminateProcess, WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR, s, w};

/// Dark Souls III's Steam app id.
const STEAM_APP_ID: &str = "374320";
const LOAD_TIMEOUT_MS: u32 = 60_000;
/// How long the game may take to reach its entry point.
const ENTRY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// `jmp` to itself: holds the game at its entry point.
const HOLD: [u8; 2] = [0xEB, 0xFE];

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Loads `dll` into the process (blocks until its DllMain returns).
unsafe fn load(process: HANDLE, dll: &Path) -> Result<(), String> {
    let path = wide(dll.as_os_str());
    let bytes = path.len() * 2;
    let remote = unsafe { VirtualAllocEx(process, None, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
    if remote.is_null() {
        return Err(format!("{}: couldn't allocate memory in the game", dll.display()));
    }
    let result = (|| {
        unsafe { WriteProcessMemory(process, remote, path.as_ptr().cast(), bytes, None) }.map_err(|e| format!("{}: {e}", dll.display()))?;
        // kernel32 is at the same address in every process of this boot session.
        let kernel32 = unsafe { GetModuleHandleW(w!("kernel32.dll")) }.map_err(|e| e.to_string())?;
        let load_library = unsafe { GetProcAddress(kernel32, s!("LoadLibraryW")) }.ok_or("LoadLibraryW not found")?;
        let start: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32 = unsafe { std::mem::transmute(load_library) };
        let thread = unsafe { CreateRemoteThread(process, None, 0, Some(start), Some(remote), 0, None) }.map_err(|e| format!("{}: {e}", dll.display()))?;
        let waited = unsafe { WaitForSingleObject(thread, LOAD_TIMEOUT_MS) };
        let mut code = 0u32;
        let _ = unsafe { GetExitCodeThread(thread, &mut code) };
        let _ = unsafe { CloseHandle(thread) };
        if waited != WAIT_OBJECT_0 {
            return Err(format!("{}: loading it took too long", dll.display()));
        }
        // The exit code is the low half of the module handle: 0 means it didn't load.
        if code == 0 {
            return Err(format!("{}: the game couldn't load it (missing dependency or not a 64-bit DLL?)", dll.display()));
        }
        Ok(())
    })();
    let _ = unsafe { VirtualFreeEx(process, remote, 0, MEM_RELEASE) };
    result
}

/// A thread context, aligned as GetThreadContext needs.
#[repr(C, align(16))]
struct Context(CONTEXT);

/// The thread's instruction pointer and registers.
fn context(thread: HANDLE) -> Result<CONTEXT, String> {
    let mut c = Context(CONTEXT { ContextFlags: CONTEXT_CONTROL_AMD64 | CONTEXT_INTEGER_AMD64, ..Default::default() });
    unsafe { GetThreadContext(thread, &mut c.0) }.map_err(|e| format!("couldn't read the game's thread: {e}"))?;
    Ok(c.0)
}

/// Runs the suspended game up to `entry` and leaves it suspended there.
unsafe fn run_to_entry(process: HANDLE, thread: HANDLE, entry: usize, log: &mut dyn FnMut(&str)) -> Result<(), String> {
    let mut original = [0u8; 2];
    unsafe { ReadProcessMemory(process, entry as *const _, original.as_mut_ptr().cast(), 2, None) }
        .map_err(|e| format!("couldn't read the game's entry point: {e}"))?;
    let mut old = PAGE_PROTECTION_FLAGS::default();
    unsafe { VirtualProtectEx(process, entry as *const _, 2, PAGE_EXECUTE_READWRITE, &mut old) }
        .map_err(|e| format!("couldn't change the game's entry point: {e}"))?;
    let write = |bytes: &[u8; 2]| -> Result<(), String> {
        unsafe { WriteProcessMemory(process, entry as *const _, bytes.as_ptr().cast(), 2, None) }
            .map_err(|e| format!("couldn't change the game's entry point: {e}"))?;
        let _ = unsafe { FlushInstructionCache(process, Some(entry as *const _), 2) };
        Ok(())
    };
    write(&HOLD)?;
    log("holding the game at its entry point");
    let started = std::time::Instant::now();
    let reached = loop {
        unsafe { ResumeThread(thread) };
        std::thread::sleep(std::time::Duration::from_millis(10));
        unsafe { SuspendThread(thread) };
        if context(thread)?.Rip as usize == entry {
            break true;
        }
        if started.elapsed() > ENTRY_TIMEOUT {
            break false;
        }
    };
    write(&original)?;
    let mut ignored = PAGE_PROTECTION_FLAGS::default();
    let _ = unsafe { VirtualProtectEx(process, entry as *const _, 2, old, &mut ignored) };
    if !reached {
        return Err("the game didn't reach its entry point in a minute".into());
    }
    log(&format!("at the entry point after {} ms", started.elapsed().as_millis()));
    Ok(())
}

/// Starts `game` with `dlls` loaded, in order, telling `log` each step. Returns the game's
/// process id.
pub fn start(game: &Path, dlls: &[PathBuf], log: &mut dyn FnMut(&str)) -> Result<u32, String> {
    let dir = game.parent().ok_or("the game path has no folder")?;
    // This process's environment plus the app id, as "name=value" strings, each ending in a 0, then a final 0.
    let mut env: Vec<u16> = Vec::new();
    for (k, v) in std::env::vars_os().filter(|(k, _)| !k.eq_ignore_ascii_case("SteamAppId")) {
        let mut entry = k;
        entry.push("=");
        entry.push(v);
        env.extend(entry.encode_wide().chain(std::iter::once(0)));
    }
    env.extend(format!("SteamAppId={STEAM_APP_ID}").encode_utf16().chain([0, 0]));
    let exe = wide(game.as_os_str());
    let mut cmdline = wide(std::ffi::OsStr::new(&format!("\"{}\"", game.display())));
    let cwd = wide(dir.as_os_str());
    let si = STARTUPINFOW { cb: size_of::<STARTUPINFOW>() as u32, ..Default::default() };
    let mut pi = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR(exe.as_ptr()),
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            Some(env.as_ptr().cast()),
            PCWSTR(cwd.as_ptr()),
            &si,
            &mut pi,
        )
    }
    .map_err(|e| {
        // ERROR_ELEVATION_REQUIRED: the game runs as administrator, this program doesn't.
        if e.code() == windows::core::HRESULT::from_win32(740) { crate::ADMIN.to_string() } else { format!("couldn't start {}: {e}", game.display()) }
    })
    .inspect_err(|e| log(&format!("failed: {e}")))?;
    log(&format!("game process {} created", pi.dwProcessId));
    let loaded = (|| {
        // A new process's main thread starts in RtlUserThreadStart with the entry point in rcx.
        let entry = context(pi.hThread)?.Rcx as usize;
        unsafe { run_to_entry(pi.hProcess, pi.hThread, entry, log) }?;
        for dll in dlls {
            log(&format!("loading {}", dll.display()));
            unsafe { load(pi.hProcess, dll) }?;
        }
        Ok::<(), String>(())
    })();
    if let Err(e) = loaded {
        log(&format!("failed: {e}"));
        let _ = unsafe { TerminateProcess(pi.hProcess, 1) };
        let _ = unsafe { CloseHandle(pi.hThread) };
        let _ = unsafe { CloseHandle(pi.hProcess) };
        return Err(e);
    }
    log("loaded; the game runs");
    unsafe { ResumeThread(pi.hThread) };
    let _ = unsafe { CloseHandle(pi.hThread) };
    let _ = unsafe { CloseHandle(pi.hProcess) };
    Ok(pi.dwProcessId)
}
