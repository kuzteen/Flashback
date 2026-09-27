// Arranque con Windows: el mismo valor `Flashback` de la clave Run que escribe el instalador (NSIS
// y MSI), así que el interruptor de Ajustes y el instalador hablan de lo mismo.
//
// Cada actualización vuelve a pasar por el instalador y reescribe el valor; por eso la elección del
// usuario se guarda en los ajustes y, si la apagó, se vuelve a quitar al arrancar.

#[cfg(target_os = "windows")]
mod reg {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
    };

    const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    // Donde el Administrador de tareas (Aplicaciones de arranque) marca una entrada como
    // desactivada sin borrarla: primer byte impar = desactivada.
    const APPROVED: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
    const NAME: PCWSTR = w!("Flashback");

    pub fn command() -> Option<String> {
        let mut buf = [0u16; 1024];
        let mut size = std::mem::size_of_val(&buf) as u32;
        let r = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, RUN, NAME, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size))
        };
        if r.is_err() {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    pub fn disabled_by_windows() -> bool {
        let mut buf = [0u8; 16];
        let mut size = buf.len() as u32;
        let r = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, APPROVED, NAME, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr().cast()), Some(&mut size))
        };
        r.is_ok() && size > 0 && buf[0] & 1 == 1
    }

    pub fn write(cmd: &str) -> Result<(), String> {
        let data: Vec<u16> = cmd.encode_utf16().chain(Some(0)).collect();
        let r = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, RUN, NAME, REG_SZ.0, Some(data.as_ptr().cast()), (data.len() * 2) as u32)
        };
        if r.is_err() {
            return Err(format!("No se pudo activar el arranque con Windows ({})", r.0));
        }
        unsafe {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, APPROVED, NAME);
        }
        Ok(())
    }

    pub fn remove() -> Result<(), String> {
        let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME) };
        unsafe {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, APPROVED, NAME);
        }
        // ERROR_FILE_NOT_FOUND: ya no estaba.
        if r.is_err() && r.0 != 2 {
            return Err(format!("No se pudo desactivar el arranque con Windows ({})", r.0));
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub fn enabled() -> bool {
    reg::command().is_some() && !reg::disabled_by_windows()
}

#[cfg(target_os = "windows")]
pub fn set(app: &tauri::AppHandle, on: bool) -> Result<(), String> {
    if on {
        // La ruta que dejó el instalador (guardada al apagarlo) antes que la del propio ejecutable,
        // que en desarrollo es la build de depuración.
        let cmd = match reg::command().or_else(|| crate::config::get_autostart_command(app)) {
            Some(c) => c,
            None => {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                format!("\"{}\" --autostart", exe.display())
            }
        };
        reg::write(&cmd)?;
    } else {
        if let Some(cmd) = reg::command() {
            crate::config::set_autostart_command(app, &cmd)?;
        }
        reg::remove()?;
    }
    crate::config::set_autostart(app, on)
}

#[cfg(target_os = "windows")]
pub fn reconcile(app: &tauri::AppHandle) {
    if crate::config::get_autostart(app) != Some(false) {
        return;
    }
    if let Some(cmd) = reg::command() {
        let _ = crate::config::set_autostart_command(app, &cmd);
        match reg::remove() {
            Ok(()) => log::info!("arranque con Windows: el instalador lo reactivó y se vuelve a quitar"),
            Err(e) => log::warn!("{e}"),
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn enabled() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn set(_app: &tauri::AppHandle, _on: bool) -> Result<(), String> {
    Err("Solo en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn reconcile(_app: &tauri::AppHandle) {}
