//! A record of the game crashing, written to `logs\crash.log` as it happens: the exception, the
//! faulting address (module + offset), the call chain (unwound from the faulting context with
//! the modules' unwind data), the code addresses found on the stack and the memory around each
//! register holding an address (a broken heap's free list shows what was written on it). The crash
//! reporter the game runs with takes crashes over and keeps nothing, so without this a crash
//! leaves no trace. Exceptions of the kinds that end the game, at most MAX a session; the
//! exception then carries on as before (Kindle doesn't handle it). They're seen first, before
//! anyone handles them, so one the game then handled would be listed too (none has been, in
//! many sessions). Verified 2026-10-06 with a deliberate access violation: its place, registers
//! and stack were written as the game went down. A fast fail or the game ending itself never
//! reaches the handler: a crash with no entry here was one of those.
//!
//! The game ending normally (its process exiting, which runs DLL detach) is noted too: a game
//! that's gone with neither line was ended without either (a fast fail or a kill).
//!
//! The handler can run with the heap or a lock broken (heap corruption, a crash inside the
//! logger), so it doesn't allocate or lock: it formats into a buffer on the stack and writes
//! it to a file opened when Kindle starts.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::{HANDLE, HMODULE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, FlushFileBuffers, OPEN_ALWAYS, WriteFile,
};
use windows::Win32::System::Diagnostics::Debug::{AddVectoredExceptionHandler, EXCEPTION_POINTERS};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::core::PCWSTR;

static FILE: AtomicIsize = AtomicIsize::new(0);
static COUNT: AtomicU32 = AtomicU32::new(0);
const MAX: u32 = 8;
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
/// How many stack slots are searched for code addresses, and how many are listed.
const STACK_SLOTS: usize = 256;
const LISTED: usize = 16;
/// How many calls of the chain are listed.
const CALLS: usize = 24;
/// The memory shown around each register holding a readable address (for a heap's broken free
/// list, what overwrote it), in rows of 16 bytes.
const ROWS_BEFORE: usize = 8;
const ROWS_AFTER: usize = 4;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn RtlLookupFunctionEntry(pc: u64, base: *mut u64, history: *mut std::ffi::c_void) -> *const std::ffi::c_void;
    fn RtlVirtualUnwind(
        kind: u32,
        base: u64,
        pc: u64,
        entry: *const std::ffi::c_void,
        ctx: *mut windows::Win32::System::Diagnostics::Debug::CONTEXT,
        data: *mut *mut std::ffi::c_void,
        frame: *mut u64,
        pointers: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
}

/// Exceptions that end the game, by name.
fn fatal(code: u32) -> Option<&'static str> {
    Some(match code {
        0xC000_0005 => "access violation",
        0xC000_0374 => "heap corruption",
        0xC000_0409 => "stack buffer overrun",
        0xC000_00FD => "stack overflow",
        0xC000_001D => "illegal instruction",
        0xC000_0094 => "integer division by zero",
        0xC000_0096 => "privileged instruction",
        _ => return None,
    })
}

/// Opens `logs\crash.log` (appending) and installs the handler.
pub fn install(logs: &Path) {
    let path: Vec<u16> = logs.join("crash.log").as_os_str().to_string_lossy().encode_utf16().chain([0]).collect();
    let file =
        unsafe { CreateFileW(PCWSTR(path.as_ptr()), FILE_APPEND_DATA.0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, None) };
    match file {
        Ok(h) => {
            FILE.store(h.0 as isize, Ordering::Release);
            // First in line: a handler before it could end the game without it seeing the crash.
            unsafe { AddVectoredExceptionHandler(1, Some(handler)) };
        }
        Err(e) => crate::warn!("crash log: can't open it ({e})"),
    }
}

/// Notes the process exiting normally (from DLL detach).
pub fn exiting() {
    let mut out = Buf { bytes: [0; 4096], len: 0 };
    let _ = write!(out, "[{:.3}] the game exited (normally)\r\n", crate::log::elapsed());
    write_out(&out);
}

fn write_out<const N: usize>(out: &Buf<N>) {
    let file = HANDLE(FILE.load(Ordering::Acquire) as *mut _);
    if file.0.is_null() {
        return;
    }
    unsafe {
        let _ = WriteFile(file, Some(&out.bytes[..out.len]), None, None);
        let _ = FlushFileBuffers(file);
    }
}

/// A fixed buffer that text is formatted into (cut off when full).
struct Buf<const N: usize = 4096> {
    bytes: [u8; N],
    len: usize,
}

/// The 16-byte rows around `addr` (the register `name`'s value), each read on its own: a row
/// that can't be read (past the end of a block's page) shows as `??`. Nothing when none can.
fn dump_around<const N: usize>(out: &mut Buf<N>, name: &str, addr: u64) {
    let first = (addr as usize & !0xF).saturating_sub(ROWS_BEFORE * 16);
    let mut rows = [[0u8; 16]; ROWS_BEFORE + ROWS_AFTER];
    let mut readable = [false; ROWS_BEFORE + ROWS_AFTER];
    for (i, row) in rows.iter_mut().enumerate() {
        readable[i] = crate::memory::read_into(first + i * 16, row);
    }
    if !readable.iter().any(|&r| r) {
        return;
    }
    let _ = write!(out, "\r\n  {name} {addr:#x}:");
    for (i, row) in rows.iter().enumerate() {
        let _ = write!(out, "\r\n    {:#x}", first + i * 16);
        for b in row {
            let _ = if readable[i] { write!(out, " {b:02x}") } else { out.write_str(" ??") };
        }
    }
}

/// The memory around each register of `ctx` holding an address, written as its own entry (its
/// own frame: the handler's stays small when this isn't called).
#[inline(never)]
fn dump_registers(ctx: &windows::Win32::System::Diagnostics::Debug::CONTEXT) {
    let mut dump = Buf { bytes: [0; 8192], len: 0 };
    let _ = dump.write_str("  memory:");
    for (name, v) in [("rax", ctx.Rax), ("rbx", ctx.Rbx), ("rcx", ctx.Rcx), ("rdx", ctx.Rdx), ("rsi", ctx.Rsi), ("rdi", ctx.Rdi)] {
        if v >= 0x10000 {
            dump_around(&mut dump, name, v);
        }
    }
    let _ = dump.write_str("\r\n");
    write_out(&dump);
}

impl<const N: usize> std::fmt::Write for Buf<N> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let take = s.len().min(self.bytes.len() - self.len);
        self.bytes[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

/// `addr` as "module+0xoffset" (the module's file name), or the bare address.
fn where_is(out: &mut Buf, addr: usize) -> bool {
    let mut module = HMODULE::default();
    let found = unsafe {
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, PCWSTR(addr as *const u16), &mut module)
    };
    if found.is_err() || module.is_invalid() {
        let _ = write!(out, "{addr:#x}");
        return false;
    }
    let mut name = [0u16; 260];
    let n = unsafe { GetModuleFileNameW(Some(module), &mut name) } as usize;
    let file = name[..n].rsplit(|&c| c == '\\' as u16).next().unwrap_or(&[]);
    for c in char::decode_utf16(file.iter().copied()) {
        let _ = out.write_char(c.unwrap_or('?'));
    }
    let _ = write!(out, "+{:#x}", addr - module.0 as usize);
    true
}

unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let (rec, ctx) = unsafe { (&*(*info).ExceptionRecord, &*(*info).ContextRecord) };
    let code = rec.ExceptionCode.0 as u32;
    let Some(name) = fatal(code) else { return EXCEPTION_CONTINUE_SEARCH };
    if COUNT.fetch_add(1, Ordering::Relaxed) >= MAX {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let mut out = Buf { bytes: [0; 4096], len: 0 };
    let _ = write!(out, "[{:.3}] {name} ({code:#010x}) in thread {} at ", crate::log::elapsed(), unsafe { GetCurrentThreadId() });
    where_is(&mut out, rec.ExceptionAddress as usize);
    if code == 0xC000_0005 && rec.NumberParameters >= 2 {
        let kind = match rec.ExceptionInformation[0] {
            0 => "reading",
            1 => "writing",
            8 => "executing",
            _ => "touching",
        };
        let _ = write!(out, ", {kind} {:#x}", rec.ExceptionInformation[1]);
    } else if rec.NumberParameters > 0 {
        let _ = out.write_str(", parameters");
        for p in &rec.ExceptionInformation[..(rec.NumberParameters as usize).min(4)] {
            let _ = write!(out, " {p:#x}");
        }
    }
    let _ = write!(
        out,
        "\r\n  rax {:#x} rbx {:#x} rcx {:#x} rdx {:#x} rsi {:#x} rdi {:#x} rsp {:#x}\r\n  calls:",
        ctx.Rax, ctx.Rbx, ctx.Rcx, ctx.Rdx, ctx.Rsi, ctx.Rdi, ctx.Rsp
    );
    // The call chain: each frame unwound with its function's unwind data (a function without
    // any is a leaf: its return address is at rsp). The game's functions have theirs (.pdata).
    let mut c = *ctx;
    for _ in 0..CALLS {
        let pc = c.Rip;
        if pc < 0x10000 {
            break;
        }
        let _ = out.write_str(" ");
        where_is(&mut out, pc as usize);
        let mut base = 0u64;
        let entry = unsafe { RtlLookupFunctionEntry(pc, &mut base, std::ptr::null_mut()) };
        if entry.is_null() {
            match crate::memory::read::<u64>(c.Rsp as usize) {
                Some(ret) => (c.Rip, c.Rsp) = (ret, c.Rsp + 8),
                None => break,
            }
        } else {
            let (mut data, mut frame) = (std::ptr::null_mut(), 0u64);
            unsafe { RtlVirtualUnwind(0, base, pc, entry, &mut c, &mut data, &mut frame, std::ptr::null_mut()) };
        }
        if c.Rip == pc {
            break;
        }
    }
    let _ = out.write_str("\r\n  stack:");
    // Values on the stack that point into a module: return addresses among them.
    let mut slots = [0u8; STACK_SLOTS * 8];
    let mut listed = 0;
    if crate::memory::read_into(ctx.Rsp as usize, &mut slots) {
        for chunk in slots.as_chunks::<8>().0 {
            let v = u64::from_le_bytes(*chunk) as usize;
            if v < 0x10000 || listed >= LISTED {
                continue;
            }
            let start = out.len;
            let _ = out.write_str(" ");
            if where_is(&mut out, v) {
                listed += 1;
            } else {
                out.len = start;
            }
        }
    }
    let _ = out.write_str("\r\n");
    write_out(&out);
    // The memory the registers point at, after the rest is written (and not for a stack overflow,
    // whose stack has no room for the buffer).
    if code != 0xC000_00FD {
        dump_registers(ctx);
    }
    EXCEPTION_CONTINUE_SEARCH
}
