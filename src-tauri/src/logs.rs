use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::OnceLock;

// Un archivo por día: quien reporta un fallo suele saber cuándo le pasó ("ayer, jugando").
const PREFIX: &str = "flashback-";
const KEEP_DAYS: usize = 7;
// Un día normal son unos KB. El tope solo salta si algo entra en bucle, y para entonces el
// principio del bucle, que es lo que explica el fallo, ya está escrito.
const MAX_DAY_BYTES: u64 = 5 * 1024 * 1024;
// Cola acotada con try_send: quien registra (a veces el hilo de captura) nunca espera al disco.
// Si el escritor se queda atrás se descarta la línea y después se anota cuántas se perdieron.
const QUEUE: usize = 512;
const CRATE: &str = env!("CARGO_CRATE_NAME");

static DIR: OnceLock<PathBuf> = OnceLock::new();
static DROPPED: AtomicU64 = AtomicU64::new(0);

struct Logger {
    tx: SyncSender<String>,
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info && ours(m.target())
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!("{} {:<5} {}: {}\n", timestamp(), r.level(), module(r.target()), r.args());
        if self.tx.try_send(line).is_err() {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn flush(&self) {}
}

// Solo lo de Flashback (y lo que manda la interfaz): Tauri, WebView2 y reqwest también usan
// `log` y llenarían el archivo de ruido.
fn ours(target: &str) -> bool {
    target == "ui" || target.strip_prefix(CRATE).is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
}

fn module(target: &str) -> &str {
    match target.strip_prefix(CRATE) {
        Some("") => "app",
        Some(rest) => rest.trim_start_matches("::"),
        None => target,
    }
}

pub fn dir() -> Option<&'static Path> {
    DIR.get().map(PathBuf::as_path)
}

pub fn file() -> Option<PathBuf> {
    dir().map(|d| day_file(d, day_of(&timestamp())))
}

fn day_file(dir: &Path, day: &str) -> PathBuf {
    dir.join(format!("{PREFIX}{day}.log"))
}

// Las líneas empiezan por la fecha (AAAA-MM-DD), así que el día sale de la propia línea y un
// mensaje encolado antes de medianoche va al archivo de su día.
fn day_of(line: &str) -> &str {
    line.get(..10).unwrap_or("")
}

pub fn init(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    if DIR.set(dir.clone()).is_err() {
        return;
    }
    install_panic_hook();
    let (tx, rx) = sync_channel(QUEUE);
    let spawned = std::thread::Builder::new()
        .name("flashback-log".into())
        .spawn(move || writer(dir, rx));
    if spawned.is_ok() && log::set_boxed_logger(Box::new(Logger { tx })).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

fn writer(dir: PathBuf, rx: Receiver<String>) {
    let mut out = Out::new(dir);
    while let Ok(line) = rx.recv() {
        out.write(&line);
        while let Ok(line) = rx.try_recv() {
            out.write(&line);
        }
        let lost = DROPPED.swap(0, Ordering::Relaxed);
        if lost > 0 {
            out.write(&format!("{} WARN  logs: {lost} líneas descartadas (cola llena)\n", timestamp()));
        }
        // Por lote y no por línea: un cierre inesperado pierde como mucho lo que llegó a la vez.
        out.flush();
    }
}

struct Out {
    dir: PathBuf,
    day: String,
    file: Option<BufWriter<File>>,
    len: u64,
    full: bool,
}

impl Out {
    fn new(dir: PathBuf) -> Out {
        Out { dir, day: String::new(), file: None, len: 0, full: false }
    }

    fn write(&mut self, line: &str) {
        let day = day_of(line);
        if day != self.day {
            self.switch(day);
        }
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        if self.full {
            return;
        }
        if self.len + line.len() as u64 > MAX_DAY_BYTES {
            self.full = true;
            let note = format!(
                "{} WARN  logs: límite de {} MB del día alcanzado; no se escribe más hasta mañana\n",
                timestamp(),
                MAX_DAY_BYTES >> 20
            );
            self.put(&note);
            return;
        }
        self.put(line);
    }

    fn put(&mut self, line: &str) {
        if let Some(f) = self.file.as_mut() {
            if f.write_all(line.as_bytes()).is_ok() {
                self.len += line.len() as u64;
            }
        }
    }

    fn switch(&mut self, day: &str) {
        self.flush();
        let file = append(&day_file(&self.dir, day));
        self.len = file.as_ref().and_then(|f| f.metadata().ok()).map_or(0, |m| m.len());
        self.file = file.map(BufWriter::new);
        self.day = day.to_string();
        self.full = false;
        prune(&self.dir, KEEP_DAYS);
    }

    fn flush(&mut self) {
        if let Some(f) = self.file.as_mut() {
            let _ = f.flush();
        }
    }
}

fn append(path: &Path) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

// La fecha va en formato AAAA-MM-DD, así que el orden alfabético es el cronológico.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut days: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(".log"))
        })
        .collect();
    days.sort();
    let excess = days.len().saturating_sub(keep);
    for old in &days[..excess] {
        let _ = std::fs::remove_file(old);
    }
}

// Se escribe aquí mismo y no por la cola: tras un panic el proceso puede morir antes de que el
// escritor lo recoja. Los panics contenidos del pipeline de captura también pasan por aquí.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let payload = info.payload();
        let msg = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("?");
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let line = format!(
            "{} ERROR panic: hilo '{}' en {at}: {msg}\n",
            timestamp(),
            thread.name().unwrap_or("?")
        );
        if let Some(mut f) = file().and_then(|p| append(&p)) {
            let _ = f.write_all(line.as_bytes());
        }
        default(info);
    }));
}

#[cfg(target_os = "windows")]
fn timestamp() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

#[cfg(not(target_os = "windows"))]
fn timestamp() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}.{:03}", d.as_secs(), d.subsec_millis())
}

// Contexto para leer el resto del archivo: sin versión de Windows, GPU y driver casi ningún fallo
// de captura o export se puede explicar.
#[cfg(target_os = "windows")]
pub fn log_system(version: &str) {
    log::info!("Flashback {version} en {}", windows_version().unwrap_or_else(|| "Windows ?".into()));
    for gpu in gpus() {
        log::info!("GPU: {gpu}");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn log_system(version: &str) {
    log::info!("Flashback {version}");
}

#[cfg(target_os = "windows")]
pub fn windows_build() -> Option<u32> {
    static BUILD: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    *BUILD.get_or_init(|| reg_string("CurrentBuild")?.parse().ok())
}

#[cfg(not(target_os = "windows"))]
pub fn windows_build() -> Option<u32> {
    None
}

#[cfg(target_os = "windows")]
fn windows_version() -> Option<String> {
    let build = windows_build()?;
    let name = if build >= 22000 { "Windows 11" } else { "Windows 10" };
    let display = reg_string("DisplayVersion").unwrap_or_default();
    let ubr = reg_dword("UBR").map(|u| format!(".{u}")).unwrap_or_default();
    Some(format!("{name} {display} (build {build}{ubr})"))
}

#[cfg(target_os = "windows")]
const NT_KEY: windows::core::PCWSTR = windows::core::w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");

#[cfg(target_os = "windows")]
fn reg_string(value: &str) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let name: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let mut buf = [0u16; 128];
    let mut size = std::mem::size_of_val(&buf) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            NT_KEY,
            windows::core::PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if r.is_err() {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(target_os = "windows")]
fn reg_dword(value: &str) -> Option<u32> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD};
    let name: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let mut out = 0u32;
    let mut size = 4u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            NT_KEY,
            windows::core::PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut out as *mut u32).cast()),
            Some(&mut size),
        )
    };
    r.is_ok().then_some(out)
}

#[cfg(target_os = "windows")]
fn gpus() -> Vec<String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIDevice, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut i = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(i) } {
        i += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let name = String::from_utf16_lossy(&desc.Description);
        let name = name.trim_end_matches('\0');
        let vram = desc.DedicatedVideoMemory as f64 / (1u64 << 30) as f64;
        let driver = unsafe { adapter.CheckInterfaceSupport(&IDXGIDevice::IID) }
            .map(|v| {
                let v = v as u64;
                format!("{}.{}.{}.{}", v >> 48, (v >> 32) & 0xFFFF, (v >> 16) & 0xFFFF, v & 0xFFFF)
            })
            .unwrap_or_else(|_| "?".into());
        let screens = screens(&adapter);
        let screens = if screens.is_empty() { String::new() } else { format!(", pantallas: {}", screens.join(", ")) };
        out.push(format!("{name} ({vram:.1} GB, driver {driver}){screens}"));
    }
    out
}

#[cfg(target_os = "windows")]
fn screens(adapter: &windows::Win32::Graphics::Dxgi::IDXGIAdapter1) -> Vec<String> {
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    let mut out = Vec::new();
    let mut i = 0;
    while let Ok(output) = unsafe { adapter.EnumOutputs(i) } {
        i += 1;
        let Ok(desc) = (unsafe { output.GetDesc() }) else { continue };
        let r = desc.DesktopCoordinates;
        let mut mode = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
        let found = unsafe {
            EnumDisplaySettingsW(windows::core::PCWSTR(desc.DeviceName.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode)
        };
        let hz = if found.as_bool() { format!(" @ {} Hz", mode.dmDisplayFrequency) } else { String::new() };
        out.push(format!("{}x{}{hz}", r.right - r.left, r.bottom - r.top));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_flashback_targets_are_kept() {
        assert!(ours(CRATE));
        assert!(ours(&format!("{CRATE}::capture::win")));
        assert!(ours("ui"));
        assert!(!ours("tauri::manager"));
        assert!(!ours(&format!("{CRATE}_other")));
        assert_eq!(module(&format!("{CRATE}::capture::win")), "capture::win");
        assert_eq!(module(CRATE), "app");
        assert_eq!(module("ui"), "ui");
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fb_logs_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn each_day_gets_its_own_file() {
        let dir = temp_dir("days");
        let mut out = Out::new(dir.clone());
        out.write("2026-09-25 23:59:59.900 INFO  app: antes\n");
        out.write("2026-09-26 00:00:00.100 INFO  app: después\n");
        out.flush();
        let read = |day| std::fs::read_to_string(day_file(&dir, day)).unwrap();
        assert!(read("2026-09-25").contains("antes"));
        assert!(read("2026-09-26").contains("después"));
        assert!(!read("2026-09-26").contains("antes"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_day_stops_growing_at_the_limit() {
        let dir = temp_dir("cap");
        let line = format!("2026-09-26 12:00:00.000 INFO  app: {}\n", "x".repeat(1 << 20));
        let mut out = Out::new(dir.clone());
        for _ in 0..8 {
            out.write(&line);
        }
        out.flush();
        let text = std::fs::read_to_string(day_file(&dir, "2026-09-26")).unwrap();
        assert_eq!(text.matches("app: x").count(), 4);
        assert!(text.ends_with("hasta mañana\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_last_days_are_kept() {
        let dir = temp_dir("prune");
        for d in 1..=9 {
            std::fs::write(day_file(&dir, &format!("2026-09-{d:02}")), b"x").unwrap();
        }
        std::fs::write(dir.join("otro.txt"), b"x").unwrap();
        prune(&dir, 7);
        assert!(!day_file(&dir, "2026-09-01").exists());
        assert!(!day_file(&dir, "2026-09-02").exists());
        assert!(day_file(&dir, "2026-09-03").exists());
        assert!(day_file(&dir, "2026-09-09").exists());
        assert!(dir.join("otro.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
