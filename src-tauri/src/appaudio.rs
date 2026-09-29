// Audio de procesos concretos: el juego y las apps que elige el usuario, cada uno en su propia pista,
// en lugar del loopback de todo el sistema. Cada proceso se captura con el loopback de procesos de
// WASAPI (incluye sus hijos: Discord y los navegadores suenan desde uno) y se coloca por timestamp,
// no por orden de llegada. Cada paquete va además al bus de la pista de mezcla del clip.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

// Margen para que lleguen los paquetes de todos los procesos de una app antes de cerrar un bloque.
const LATENCY: i64 = 4_800;
// Windows 10 no tiene loopback de procesos: llegó con la build 20348.
const MIN_BUILD: u32 = 20_348;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct AudioApp {
    pub exe: String,
    pub name: String,
    pub path: String,
}

// Fuente de una pista: el juego detectado en cada momento o un ejecutable (en minúsculas).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Target {
    Game,
    Exe(String),
}

static CONFIG: RwLock<Option<Vec<AudioApp>>> = RwLock::new(None);

pub fn supported() -> bool {
    crate::logs::windows_build().is_some_and(|b| b >= MIN_BUILD)
}

// El modo y la lista deciden qué pistas lleva el clip, y un MP4 declara sus pistas al empezar: cambiar
// cualquiera de los dos re-arma el replay.
pub fn configure(mode: &str, apps: &[AudioApp]) {
    let list = match mode {
        "game" => Some(preset_apps()),
        "apps" => Some(apps.to_vec()),
        _ => None,
    };
    *CONFIG.write().unwrap_or_else(|e| e.into_inner()) = list.filter(|_| supported());
}

// Modo "Juego y apps": la lista fija que la interfaz enseña en `PRESET_APPS`.
pub fn preset_apps() -> Vec<AudioApp> {
    [("Discord.exe", "Discord"), ("Spotify.exe", "Spotify")]
        .into_iter()
        .map(|(exe, name)| AudioApp { exe: exe.into(), name: name.into(), path: String::new() })
        .collect()
}

// Apps que llevan pista propia, si el modo no es "Todo el PC".
pub fn configured() -> Option<Vec<AudioApp>> {
    CONFIG.read().unwrap_or_else(|e| e.into_inner()).clone()
}

struct Proc {
    pid: u32,
    parent: u32,
    exe: String,
}

// Procesos a capturar y la pista de cada uno. De cada app se toma la raíz de sus procesos (el que no
// tiene por padre otro del mismo exe) y el loopback incluye el resto del árbol. Un objetivo que ya
// cuelga de otro se quita, o su audio sonaría dos veces (un juego lanzado desde una app de la lista
// suena en la pista de esa app).
fn targets(procs: &[Proc], game: Option<u32>, slots: &[Target]) -> Vec<(u32, usize)> {
    let by_pid: HashMap<u32, &Proc> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut cand: Vec<(u32, usize)> = Vec::new();
    if let (Some(slot), Some(g)) = (slots.iter().position(|t| *t == Target::Game), game) {
        if by_pid.contains_key(&g) {
            cand.push((g, slot));
        }
    }
    for p in procs {
        let Some(slot) = slots.iter().position(|t| matches!(t, Target::Exe(e) if *e == p.exe)) else { continue };
        if by_pid.get(&p.parent).is_none_or(|pp| pp.exe != p.exe) {
            cand.push((p.pid, slot));
        }
    }
    let mut seen = HashSet::new();
    cand.retain(|(pid, _)| seen.insert(*pid));
    let covered = |pid: u32| {
        let mut cur = by_pid.get(&pid).map(|p| p.parent);
        for _ in 0..64 {
            match cur {
                Some(pp) if pp != 0 && pp != pid => {
                    if seen.contains(&pp) {
                        return true;
                    }
                    cur = by_pid.get(&pp).map(|p| p.parent);
                }
                _ => return false,
            }
        }
        false
    };
    cand.retain(|(pid, _)| !covered(*pid));
    cand.sort_unstable();
    cand
}

// Programa al que pertenece una sesión de audio. WebView2 es el motor web con el que otras apps
// pintan su ventana (el propio editor de Flashback, Teams, Widgets): su sesión se atribuye a la app
// que lo abrió, que es la que tiene sentido elegir y cuyo árbol de procesos ya incluye su audio. Lo
// que cuelga de Flashback no se ofrece.
fn session_owner(pid: u32, procs: &[Proc], own: u32) -> Option<u32> {
    let by_pid: HashMap<u32, &Proc> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut owner = pid;
    let mut cur = pid;
    for _ in 0..64 {
        if cur == own {
            return None;
        }
        let Some(p) = by_pid.get(&cur) else { break };
        if owner == cur && p.exe == "msedgewebview2.exe" {
            owner = p.parent;
        }
        if p.parent == 0 || p.parent == cur {
            break;
        }
        cur = p.parent;
    }
    Some(owner)
}

#[derive(Serialize, Clone, Debug)]
pub struct AudioAppInfo {
    pub exe: String,
    pub name: String,
    pub path: String,
    pub icon: Option<String>,
    pub active: bool,
}

#[cfg(target_os = "windows")]
pub use win::{app_from_path, audio_sessions, run};

// Iconos de las apps guardados en app-data por nombre de ejecutable: un clip grabado con una app
// sigue enseñando su icono aunque se quite de la lista, se actualice (Discord cambia de carpeta en
// cada versión) o se desinstale. Son unos KB por app y no van en la caché que se limpia.
fn icon_file(dir: &std::path::Path, exe: &str) -> std::path::PathBuf {
    let name: String = exe.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' }).collect();
    dir.join(format!("{name}.png"))
}

fn data_url(png: &[u8]) -> String {
    use base64::Engine;
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

// Icono guardado de `exe`; si aún no está, lo saca de `path` o de un proceso abierto de esa app y lo
// guarda. Requiere COM inicializado.
pub fn app_icon(dir: &std::path::Path, exe: &str, path: Option<&str>) -> Option<String> {
    let file = icon_file(dir, exe);
    if let Ok(png) = std::fs::read(&file) {
        return Some(data_url(&png));
    }
    #[cfg(target_os = "windows")]
    {
        let png = path
            .filter(|p| !p.is_empty())
            .and_then(win::icon_png)
            .or_else(|| win::running_path(exe).and_then(|p| win::icon_png(&p)))?;
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&file, &png);
        Some(data_url(&png))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        None
    }
}

// Al armar el replay con la lista de apps: así toda app con la que se grabó deja su icono guardado.
pub fn remember_icons(dir: std::path::PathBuf, apps: Vec<AudioApp>) {
    let missing: Vec<AudioApp> = apps.into_iter().filter(|a| !icon_file(&dir, &a.exe).exists()).collect();
    if missing.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new().name("flashback-app-icons".into()).spawn(move || {
        #[cfg(target_os = "windows")]
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        for a in missing {
            app_icon(&dir, &a.exe, Some(&a.path));
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub fn audio_sessions() -> Vec<AudioAppInfo> {
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
pub fn app_from_path(_path: &str) -> Option<AudioAppInfo> {
    None
}

#[cfg(target_os = "windows")]
mod win {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use windows::core::{implement, Interface, Ref, Result, HRESULT, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, SIZE};
    use windows::Win32::Graphics::Gdi::{DeleteObject, HPALETTE};
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatPng, IWICImagingFactory, WICBitmapEncoderNoCache,
        WICBitmapUseAlpha,
    };
    use windows::Win32::Media::Audio::{
        eRender, ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
        IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient,
        IAudioClient, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, AudioSessionStateActive, DEVICE_STATE_ACTIVE,
        PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
        WAVE_FORMAT_PCM,
    };
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoCreateInstance, IStream, BLOB, CLSCTX_ALL, CLSCTX_INPROC_SERVER, STREAM_SEEK_CUR};
    use windows::Win32::System::Threading::CreateEventW;
    use windows::Win32::System::Variant::VT_BLOB;
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};

    use super::{session_owner, targets, AudioAppInfo, Proc, Target, LATENCY};
    use crate::audiomix::{now_hns, Cursor, Emit, Mix, MixBus, CH, CHANNELS, RATE};

    const RETRY: Duration = Duration::from_secs(10);

    #[implement(IActivateAudioInterfaceCompletionHandler)]
    struct Activated(Mutex<Option<Sender<()>>>);

    impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
        fn ActivateCompleted(&self, _op: Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> Result<()> {
            if let Some(tx) = self.0.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = tx.send(());
            }
            Ok(())
        }
    }

    struct Source {
        pid: u32,
        exe: String,
        slot: usize,
        client: IAudioClient,
        capture: IAudioCaptureClient,
        event: HANDLE,
        cursor: Cursor,
    }

    impl Drop for Source {
        fn drop(&mut self) {
            unsafe {
                let _ = self.client.Stop();
                let _ = CloseHandle(self.event);
            }
        }
    }

    impl Source {
        fn open(pid: u32, exe: String, slot: usize) -> Result<Source> {
            let params = AUDIOCLIENT_ACTIVATION_PARAMS {
                ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
                Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                    ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                        TargetProcessId: pid,
                        ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                    },
                },
            };
            // Sin soltarlo: su Drop llama a PropVariantClear, que liberaría el blob, y el blob
            // apunta a `params`, en la pila.
            let mut prop = std::mem::ManuallyDrop::new(PROPVARIANT::default());
            unsafe {
                let inner = &mut *prop.Anonymous.Anonymous;
                inner.vt = VT_BLOB;
                inner.Anonymous.blob = BLOB {
                    cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                    pBlobData: &params as *const _ as *mut u8,
                };
            }
            let (tx, rx) = channel();
            let handler: IActivateAudioInterfaceCompletionHandler = Activated(Mutex::new(Some(tx))).into();
            let op = unsafe {
                ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*prop), &handler)?
            };
            rx.recv_timeout(Duration::from_secs(3)).map_err(|_| windows::core::Error::from_hresult(E_FAIL))?;
            let mut hr = HRESULT(0);
            let mut unk = None;
            unsafe { op.GetActivateResult(&mut hr, &mut unk)? };
            hr.ok()?;
            let client: IAudioClient = unk.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?.cast()?;

            let align = CHANNELS * 2;
            let fmt = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: CHANNELS,
                nSamplesPerSec: RATE,
                nAvgBytesPerSec: RATE * align as u32,
                nBlockAlign: align,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            let flags = AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 20_000_000, 0, &fmt, None)? };
            let event = unsafe { CreateEventW(None, false, false, PCWSTR::null())? };
            let started = (|| -> Result<IAudioCaptureClient> {
                unsafe {
                    client.SetEventHandle(event)?;
                    let capture: IAudioCaptureClient = client.GetService()?;
                    client.Start()?;
                    Ok(capture)
                }
            })();
            match started {
                Ok(capture) => Ok(Source { pid, exe, slot, client, capture, event, cursor: Cursor::default() }),
                Err(e) => {
                    unsafe { let _ = CloseHandle(event); }
                    Err(e)
                }
            }
        }

        fn drain(&mut self, mix: &mut Mix, bus: Option<&MixBus>, scratch: &mut Vec<i16>) -> bool {
            loop {
                let Ok(packet) = (unsafe { self.capture.GetNextPacketSize() }) else { return false };
                if packet == 0 {
                    return true;
                }
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                let mut qpc = 0u64;
                if unsafe { self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc)) }.is_err() {
                    return false;
                }
                let samples = frames as usize * CH;
                scratch.clear();
                if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                    scratch.resize(samples, 0);
                } else {
                    scratch.extend_from_slice(unsafe { std::slice::from_raw_parts(data as *const i16, samples) });
                }
                if unsafe { self.capture.ReleaseBuffer(frames) }.is_err() {
                    return false;
                }
                let stamp = mix.frame_at(qpc as i64);
                self.cursor.place(mix, stamp, scratch);
                if let Some(bus) = bus {
                    bus.push(self.pid as u64, qpc as i64, scratch, CH);
                }
            }
        }
    }

    // Bucle de las pistas de apps: repasa cada segundo qué procesos toca capturar y cada ~10 ms recoge
    // sus paquetes y entrega los bloques ya cerrados de cada pista. Una app que se abre, se cierra o
    // se reinicia entra y sale sola, sin tocar el vídeo; mientras no está abierta su pista no escribe
    // nada (queda un hueco, como el loopback en silencio).
    pub fn run(
        stop: &AtomicBool,
        slots: &[Target],
        emit: &mut [Emit<'_>],
        bus: Option<&MixBus>,
    ) {
        let latency_hns = LATENCY * 10_000_000 / RATE as i64;
        let origin = now_hns() - latency_hns;
        let mut mixes: Vec<Mix> = slots.iter().map(|_| Mix::new(origin)).collect();
        let mut sources: Vec<Source> = Vec::new();
        let mut failed: HashMap<u32, Instant> = HashMap::new();
        let mut scratch: Vec<i16> = Vec::new();
        let mut last_scan: Option<Instant> = None;

        while !stop.load(Ordering::SeqCst) {
            if last_scan.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
                last_scan = Some(Instant::now());
                let procs = processes();
                let want = targets(&procs, crate::detect::current_game_pid(), slots);
                sources.retain(|s| want.contains(&(s.pid, s.slot)));
                failed.retain(|pid, at| want.iter().any(|(p, _)| p == pid) && at.elapsed() < RETRY);
                for (pid, slot) in want {
                    if sources.iter().any(|s| s.pid == pid) || failed.contains_key(&pid) {
                        continue;
                    }
                    let exe = procs.iter().find(|p| p.pid == pid).map(|p| p.exe.clone()).unwrap_or_default();
                    match Source::open(pid, exe.clone(), slot) {
                        Ok(s) => {
                            log::info!("audio de apps: capturando {exe} (pid {pid})");
                            sources.push(s);
                        }
                        Err(e) => {
                            log::warn!("audio de apps: no se pudo capturar {exe} (pid {pid}): {e:?}");
                            failed.insert(pid, Instant::now());
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(10));
            sources.retain_mut(|s| {
                let alive = s.drain(&mut mixes[s.slot], bus, &mut scratch);
                if !alive {
                    log::info!("audio de apps: {} (pid {}) dejó de responder", s.exe, s.pid);
                }
                alive
            });
            let now = now_hns();
            for (i, mix) in mixes.iter_mut().enumerate() {
                let upto = mix.frame_at(now) - LATENCY;
                if sources.iter().any(|s| s.slot == i) {
                    mix.drain(upto, &mut *emit[i]);
                } else {
                    mix.skip(upto);
                }
            }
        }
    }

    fn processes() -> Vec<Proc> {
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };
        let mut out = Vec::new();
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
            let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                    out.push(Proc {
                        pid: entry.th32ProcessID,
                        parent: entry.th32ParentProcessID,
                        exe: String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase(),
                    });
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
        out
    }

    fn process_path(pid: u32) -> Option<String> {
        use windows::core::PWSTR;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            let res = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size);
            let _ = CloseHandle(handle);
            res.ok()?;
            Some(String::from_utf16_lossy(&buf[..size as usize]))
        }
    }

    // Apps con una sesión de audio abierta en cualquier salida, las que suenan ahora primero. El juego
    // en curso no se ofrece: ya tiene su propia pista.
    pub fn audio_sessions() -> Vec<AudioAppInfo> {
        let own = std::process::id();
        let game_pid = crate::detect::current_game_pid();
        let game_exe = game_pid.and_then(process_path).and_then(|p| describe(&p)).map(|g| g.exe.to_lowercase());
        let procs = processes();
        let mut seen: HashMap<String, AudioAppInfo> = HashMap::new();
        let Ok(enumerator) = (unsafe { CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) })
        else {
            return Vec::new();
        };
        let Ok(devices) = (unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }) else { return Vec::new() };
        let count = unsafe { devices.GetCount() }.unwrap_or(0);
        for i in 0..count {
            let Ok(device) = (unsafe { devices.Item(i) }) else { continue };
            let Ok(manager) = (unsafe { device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }) else { continue };
            let Ok(sessions) = (unsafe { manager.GetSessionEnumerator() }) else { continue };
            let n = unsafe { sessions.GetCount() }.unwrap_or(0);
            for j in 0..n {
                let Ok(control) = (unsafe { sessions.GetSession(j) }) else { continue };
                let Ok(control) = control.cast::<IAudioSessionControl2>() else { continue };
                if unsafe { control.IsSystemSoundsSession() } == windows::Win32::Foundation::S_OK {
                    continue;
                }
                let Ok(pid) = (unsafe { control.GetProcessId() }) else { continue };
                let Some(pid) = session_owner(pid, &procs, own) else { continue };
                if pid == 0 || Some(pid) == game_pid {
                    continue;
                }
                let active = unsafe { control.GetState() }.is_ok_and(|s| s == AudioSessionStateActive);
                let Some(path) = process_path(pid) else { continue };
                let Some(mut info) = describe(&path) else { continue };
                if game_exe.as_deref() == Some(info.exe.to_lowercase().as_str()) {
                    continue;
                }
                info.active = active;
                seen.entry(info.exe.to_lowercase())
                    .and_modify(|e| e.active |= active)
                    .or_insert(info);
            }
        }
        let mut list: Vec<AudioAppInfo> = seen.into_values().collect();
        list.sort_by(|a, b| b.active.cmp(&a.active).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        for app in &mut list {
            app.icon = icon(&app.path);
        }
        list
    }

    pub fn app_from_path(path: &str) -> Option<AudioAppInfo> {
        let mut info = describe(path)?;
        info.icon = icon(path);
        Some(info)
    }

    fn describe(path: &str) -> Option<AudioAppInfo> {
        let file = std::path::Path::new(path);
        let exe = file.file_name()?.to_string_lossy().into_owned();
        let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| exe.clone());
        let name = file_description(path).filter(|d| !d.trim().is_empty()).unwrap_or(stem);
        Some(AudioAppInfo { exe, name: name.trim().to_string(), path: path.to_string(), icon: None, active: false })
    }

    fn file_description(path: &str) -> Option<String> {
        let wide = HSTRING::from(path);
        unsafe {
            let size = GetFileVersionInfoSizeW(&wide, None);
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            GetFileVersionInfoW(&wide, None, size, data.as_mut_ptr().cast()).ok()?;
            let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let mut len = 0u32;
            let mut langs: Vec<(u16, u16)> = Vec::new();
            if VerQueryValueW(data.as_ptr().cast(), &HSTRING::from("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool()
                && len >= 4
            {
                let pairs = std::slice::from_raw_parts(ptr as *const u16, len as usize / 2);
                langs.extend(pairs.chunks_exact(2).map(|c| (c[0], c[1])));
            }
            langs.push((0x0409, 0x04b0));
            langs.push((0x0409, 0x04e4));
            for (lang, cp) in langs {
                let key = HSTRING::from(format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"));
                if VerQueryValueW(data.as_ptr().cast(), &key, &mut ptr, &mut len).as_bool() && len > 1 {
                    let s = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                    return Some(String::from_utf16_lossy(&s[..end]));
                }
            }
            None
        }
    }

    pub fn icon(path: &str) -> Option<String> {
        icon_png(path).map(|png| super::data_url(&png))
    }

    // Ruta del ejecutable de un proceso abierto de esa app.
    pub fn running_path(exe: &str) -> Option<String> {
        let exe = exe.to_lowercase();
        processes().iter().filter(|p| p.exe == exe).find_map(|p| process_path(p.pid))
    }

    pub fn icon_png(path: &str) -> Option<Vec<u8>> {
        // El shell no resuelve rutas con barras normales.
        let path = path.replace('/', "\\");
        unsafe {
            let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
            let bitmap = item.GetImage(SIZE { cx: 64, cy: 64 }, SIIGBF_ICONONLY).ok()?;
            let png = (|| -> Result<Vec<u8>> {
                let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
                let source = factory.CreateBitmapFromHBITMAP(bitmap, HPALETTE::default(), WICBitmapUseAlpha)?;
                let mut mem = vec![0u8; 64 * 64 * 4 + 16 * 1024];
                let stream = factory.CreateStream()?;
                stream.InitializeFromMemory(&mem)?;
                let encoder = factory.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
                encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
                let mut frame = None;
                encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
                let frame = frame.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
                frame.Initialize(None)?;
                frame.WriteSource(&source, std::ptr::null())?;
                frame.Commit()?;
                encoder.Commit()?;
                let mut end = 0u64;
                stream.cast::<IStream>()?.Seek(0, STREAM_SEEK_CUR, Some(&mut end))?;
                drop(stream);
                mem.truncate(end as usize);
                Ok(mem)
            })();
            let _ = DeleteObject(bitmap.into());
            png.ok()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

        #[test]
        fn process_loopback_opens_and_the_track_keeps_flowing() {
            if !super::super::supported() {
                return;
            }
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            Source::open(std::process::id(), "test".into(), 0).expect("loopback de proceso");
            let stop = AtomicBool::new(false);
            let mut blocks = 0;
            let bus = MixBus::new(now_hns());
            std::thread::scope(|s| {
                s.spawn(|| {
                    std::thread::sleep(Duration::from_millis(600));
                    stop.store(true, Ordering::SeqCst);
                });
                let mut emit: Vec<Emit> = vec![Box::new(|_, _, _| blocks += 1)];
                run(&stop, &[Target::Exe("no-existe.exe".into())], &mut emit, Some(&bus));
            });
            assert_eq!(blocks, 0, "una app que no está abierta no escribe nada");
        }

        // Suena un tono desde este mismo proceso: el loopback de su árbol tiene que traerlo y el
        // de otro proceso no.
        #[test]
        fn captures_only_the_audio_of_the_target_process() {
            use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FLAGS, SND_MEMORY, SND_NODEFAULT};
            if !super::super::supported() {
                return;
            }
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let tone: Vec<i16> =
                (0..RATE as usize).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 12_000.0) as i16).collect();
            let data: Vec<u8> = tone.iter().flat_map(|s| s.to_le_bytes()).collect();
            let mut wav = b"RIFF".to_vec();
            wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&RATE.to_le_bytes());
            wav.extend_from_slice(&(RATE * 2).to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
            wav.extend_from_slice(&data);

            let peak = |pid: u32| {
                let mut src = Source::open(pid, String::new(), 0).expect("loopback de proceso");
                let mut mix = Mix::new(now_hns());
                let bus = MixBus::new(now_hns());
                let mut scratch = Vec::new();
                let mut peak = 0i32;
                let mut bus_peak = 0i32;
                let t0 = Instant::now();
                while t0.elapsed() < Duration::from_millis(500) {
                    std::thread::sleep(Duration::from_millis(20));
                    assert!(src.drain(&mut mix, Some(&bus), &mut scratch));
                }
                let level = |b: Vec<u8>| b.chunks_exact(2).map(|c| (i16::from_le_bytes([c[0], c[1]]) as i32).abs()).max().unwrap_or(0);
                mix.drain(mix.frame_at(now_hns()), &mut |b, _, _| peak = peak.max(level(b)));
                bus.drain(now_hns(), &mut |b, _, _| bus_peak = bus_peak.max(level(b)));
                assert!((peak - bus_peak).abs() < 100, "pista {peak} y mezcla {bus_peak}");
                peak
            };
            unsafe {
                let _ = PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
            }
            std::thread::sleep(Duration::from_millis(100));
            let own = peak(std::process::id());
            // Un proceso propio que no suena: el árbol de explorer.exe incluye las apps abiertas y
            // podría traer lo que esté sonando en el equipo.
            let mut quiet = std::process::Command::new("ping").args(["-n", "5", "127.0.0.1"]).spawn().unwrap();
            let other = peak(quiet.id());
            let _ = quiet.kill();
            let _ = quiet.wait();
            unsafe {
                let _ = PlaySoundW(PCWSTR::null(), None, SND_FLAGS(0));
            }
            assert!(own > 5_000, "pico propio {own}");
            assert!(other < 1_000, "pico ajeno {other}");
        }

        #[test]
        fn describes_an_exe_by_its_file_description() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let app = app_from_path(r"C:\Windows\explorer.exe").expect("explorer");
            assert_eq!(app.exe, "explorer.exe");
            assert!(!app.name.is_empty());
            assert!(app.icon.is_some_and(|i| i.starts_with("data:image/png;base64,")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, exe: &str) -> Proc {
        Proc { pid, parent, exe: exe.into() }
    }

    fn exe(e: &str) -> Target {
        Target::Exe(e.into())
    }

    #[test]
    fn apps_resolve_to_their_root_process() {
        let procs = [
            p(1, 0, "explorer.exe"),
            p(10, 1, "discordptb.exe"),
            p(11, 10, "discordptb.exe"),
            p(12, 10, "discordptb.exe"),
            p(20, 1, "discord.exe"),
            p(21, 20, "discord.exe"),
        ];
        assert_eq!(targets(&procs, None, &[exe("discordptb.exe")]), vec![(10, 0)]);
    }

    #[test]
    fn each_process_goes_to_its_own_track() {
        let procs = [p(1, 0, "explorer.exe"), p(5, 1, "steam.exe"), p(50, 5, "game.exe"), p(60, 1, "spotify.exe")];
        let slots = [Target::Game, exe("spotify.exe")];
        assert_eq!(targets(&procs, Some(50), &slots), vec![(50, 0), (60, 1)]);
        let slots = [Target::Game, exe("steam.exe")];
        assert_eq!(targets(&procs, Some(50), &slots), vec![(5, 1)]);
        assert_eq!(targets(&procs, Some(99), &[Target::Game]), Vec::<(u32, usize)>::new());
    }

    #[test]
    fn a_game_also_on_the_list_is_captured_once() {
        let procs = [p(1, 0, "explorer.exe"), p(50, 1, "game.exe")];
        assert_eq!(targets(&procs, Some(50), &[Target::Game, exe("game.exe")]), vec![(50, 0)]);
    }

    #[test]
    fn webview2_sessions_belong_to_the_app_that_hosts_them() {
        let procs = [
            p(1, 0, "explorer.exe"),
            p(100, 1, "flashback.exe"),
            p(101, 100, "msedgewebview2.exe"),
            p(102, 101, "msedgewebview2.exe"),
            p(200, 1, "ms-teams.exe"),
            p(201, 200, "msedgewebview2.exe"),
            p(202, 201, "msedgewebview2.exe"),
            p(300, 1, "sharex.exe"),
        ];
        assert_eq!(session_owner(102, &procs, 100), None);
        assert_eq!(session_owner(202, &procs, 100), Some(200));
        assert_eq!(session_owner(300, &procs, 100), Some(300));
        assert_eq!(session_owner(100, &procs, 100), None);
    }

    #[test]
    fn a_saved_icon_outlives_the_app() {
        let dir = std::env::temp_dir().join(format!("fb_icons_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(app_icon(&dir, "NoExiste.exe", None), None);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(icon_file(&dir, "Discord.EXE"), [1u8, 2, 3]).unwrap();
        assert_eq!(app_icon(&dir, "discord.exe", Some("C:/no/existe.exe")).as_deref(), Some("data:image/png;base64,AQID"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_instances_of_an_app_share_its_track() {
        let procs = [p(1, 0, "explorer.exe"), p(7, 1, "vlc.exe"), p(8, 1, "vlc.exe")];
        assert_eq!(targets(&procs, None, &[exe("vlc.exe")]), vec![(7, 0), (8, 0)]);
    }
}
