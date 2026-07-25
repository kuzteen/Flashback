#[cfg(not(target_os = "windows"))]
pub fn drag(_hwnd: isize, _path: &str, _thumb: Option<&str>) -> Result<bool, String> {
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
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::{
        CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HBITMAP,
    };
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
        WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
        WICDecodeMetadataCacheOnLoad,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, IBindCtx, IDataObject, CLSCTX_INPROC_SERVER,
    };
    use windows::Win32::System::Ole::{
        OleInitialize, OleUninitialize, IDropSource, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK,
    };
    use windows::Win32::UI::Shell::{
        CLSID_DragDropHelper, IDragSourceHelper, IShellItem, SHCreateItemFromParsingName,
        SHDoDragDrop, BHID_DataObject, SHDRAGIMAGE,
    };

    // Alto de la imagen que sigue al cursor. Suficiente para reconocer la escena sin tapar el sitio
    // donde se va a soltar; el ancho sale del aspecto real del fotograma.
    const DRAG_IMAGE_H: u32 = 116;

    // Decodifica la miniatura cacheada del clip a un DIB de 32 bits premultiplicado, que es lo que
    // el shell espera en SHDRAGIMAGE. Best-effort en todo el camino: sin imagen el arrastre sigue
    // funcionando, solo que con el icono genérico del shell.
    unsafe fn load_drag_bitmap(path: &str) -> Option<(HBITMAP, u32, u32)> {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let decoder = factory
            .CreateDecoderFromFilename(
                &HSTRING::from(path),
                None,
                windows::Win32::Foundation::GENERIC_READ,
                WICDecodeMetadataCacheOnLoad,
            )
            .ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let mut sw = 0u32;
        let mut sh = 0u32;
        frame.GetSize(&mut sw, &mut sh).ok()?;
        if sw == 0 || sh == 0 {
            return None;
        }
        let h = DRAG_IMAGE_H.min(sh);
        let w = ((sw as u64 * h as u64 / sh as u64) as u32).max(1);

        let scaler = factory.CreateBitmapScaler().ok()?;
        scaler
            .Initialize(&frame, w, h, WICBitmapInterpolationModeFant)
            .ok()?;
        let converter = factory.CreateFormatConverter().ok()?;
        converter
            .Initialize(
                &scaler,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .ok()?;

        // Top-down (alto negativo): así el orden de filas del DIB coincide con el que entrega WIC y
        // no hay que voltear la imagen a mano.
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(bmp.into());
            return None;
        }
        let stride = w * 4;
        let buf = std::slice::from_raw_parts_mut(bits as *mut u8, (stride * h) as usize);
        if converter.CopyPixels(std::ptr::null(), stride, buf).is_err() {
            let _ = DeleteObject(bmp.into());
            return None;
        }
        Some((bmp, w, h))
    }

    // El shell solo genera imagen de arrastre por su cuenta en algunos destinos; fijarla aquí hace
    // que el fotograma del clip se vea siempre bajo el cursor en vez del icono de archivo.
    unsafe fn set_drag_image(data: &IDataObject, thumb: &str) {
        let Some((bmp, w, h)) = load_drag_bitmap(thumb) else {
            return;
        };
        let helper: Result<IDragSourceHelper, _> =
            CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER);
        let Ok(helper) = helper else {
            let _ = DeleteObject(bmp.into());
            return;
        };
        let mut img = SHDRAGIMAGE {
            sizeDragImage: windows::Win32::Foundation::SIZE {
                cx: w as i32,
                cy: h as i32,
            },
            // El cursor agarra la imagen por el centro, que es como se comporta el Explorador.
            ptOffset: POINT {
                x: (w / 2) as i32,
                y: (h / 2) as i32,
            },
            hbmpDragImage: bmp,
            crColorKey: windows::Win32::Foundation::COLORREF(0xFFFF_FFFF),
        };
        // En caso de éxito el helper se queda con el bitmap y lo libera él; si falla, es nuestro.
        if helper.InitializeFromBitmap(&mut img, data).is_err() {
            let _ = DeleteObject(bmp.into());
        }
    }

    // OleInitialize devuelve S_FALSE si el hilo ya lo tenía inicializado, pero incrementa el contador
    // igual, así que hay que deshacerlo en los dos casos. Solo cuando falla (el hilo está en MTA) no
    // hay nada que deshacer, y ahí ni se construye el guard.
    struct OleScope;

    impl Drop for OleScope {
        fn drop(&mut self) {
            unsafe { OleUninitialize() };
        }
    }

    pub fn drag(hwnd: isize, path: &str, thumb: Option<&str>) -> Result<bool, String> {
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

            if let Some(thumb) = thumb {
                set_drag_image(&data, thumb);
            }

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
