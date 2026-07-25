#[cfg(not(target_os = "windows"))]
pub fn drag(_hwnd: isize, _path: &str) -> Result<bool, String> {
    Err("El arrastre solo está disponible en Windows".into())
}

#[cfg(target_os = "windows")]
pub use win::drag;

// Arrastre OLE de un archivo hacia otra aplicación. WebView2 no puede sacar un archivo real de la
// ventana: el drag-and-drop de HTML no produce CF_HDROP, que es lo que Discord o el Explorador
// esperan. La operación tiene que nacer aquí.
#[cfg(target_os = "windows")]
mod win {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{IBindCtx, IDataObject};
    use windows::Win32::System::Ole::{
        OleInitialize, OleUninitialize, IDropSource, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK,
    };
    use windows::Win32::UI::Shell::{
        IShellItem, SHCreateItemFromParsingName, SHDoDragDrop, BHID_DataObject,
    };

    // OleInitialize devuelve S_FALSE si el hilo ya lo tenía inicializado, pero incrementa el contador
    // igual, así que hay que deshacerlo en los dos casos. Solo cuando falla (el hilo está en MTA) no
    // hay nada que deshacer, y ahí ni se construye el guard.
    struct OleScope;

    impl Drop for OleScope {
        fn drop(&mut self) {
            unsafe { OleUninitialize() };
        }
    }

    pub fn drag(hwnd: isize, path: &str) -> Result<bool, String> {
        unsafe {
            OleInitialize(None)
                .map_err(|e| format!("El hilo de UI no admite OLE: {e:?}"))?;
            let _scope = OleScope;

            // El IDataObject lo genera el shell: trae CF_HDROP y el resto de formatos que espera el
            // Explorador, y SHDoDragDrop saca de ahí la miniatura para la imagen de arrastre.
            let no_bind: Option<&IBindCtx> = None;
            let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path), no_bind)
                .map_err(|e| format!("No se pudo abrir el archivo: {e:?}"))?;
            let data: IDataObject = item
                .BindToHandler(no_bind, &BHID_DataObject)
                .map_err(|e| format!("No se pudo preparar el arrastre: {e:?}"))?;

            // SHDoDragDrop trata tanto el soltado como la cancelación como éxito, así que lo que
            // distingue un soltado real es el efecto devuelto: al cancelar viene DROPEFFECT_NONE.
            let no_source: Option<&IDropSource> = None;
            let effect = SHDoDragDrop(
                Some(HWND(hwnd as _)),
                Some(&data),
                no_source,
                DROPEFFECT_COPY | DROPEFFECT_LINK,
            )
            .map_err(|e| format!("El arrastre falló: {e:?}"))?;
            Ok(effect != DROPEFFECT(0))
        }
    }
}
