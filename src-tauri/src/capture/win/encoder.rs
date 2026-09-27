use super::*;

// Conversor de color + escalado: entrada ARGB32 a resolución de captura (in_*),
// salida NV12 a resolución objetivo (out_*). Si difieren, el Video Processor escala.
pub(super) fn build_converter(
    manager: Option<&IMFDXGIDeviceManager>,
    in_w: u32,
    in_h: u32,
    out_w: u32,
    out_h: u32,
    fps: u32,
) -> Result<IMFTransform> {
    let converter: IMFTransform =
        unsafe { CoCreateInstance(&CLSID_VIDEO_PROCESSOR_MFT, None, CLSCTX_INPROC_SERVER)? };
    // Con device manager convierte en GPU (zero-copy); sin él, en CPU (camino software).
    if let Some(manager) = manager {
        unsafe {
            let unk: windows::core::IUnknown = manager.cast()?;
            converter.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, unk.as_raw() as usize)?;
        }
    }

    let out_type = unsafe { MFCreateMediaType()? };
    unsafe {
        out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        out_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        out_type.SetUINT64(&MF_MT_FRAME_SIZE, pack2(out_w, out_h))?;
        out_type.SetUINT64(&MF_MT_FRAME_RATE, pack2(fps, 1))?;
        out_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack2(1, 1))?;
    }

    let in_type = unsafe { MFCreateMediaType()? };
    unsafe {
        in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        in_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32)?;
        in_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        in_type.SetUINT64(&MF_MT_FRAME_SIZE, pack2(in_w, in_h))?;
        in_type.SetUINT64(&MF_MT_FRAME_RATE, pack2(fps, 1))?;
        in_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack2(1, 1))?;
    }

    unsafe {
        converter.SetInputType(0, &in_type, 0)?;
        converter.SetOutputType(0, &out_type, 0)?;
    }
    Ok(converter)
}

// Selecciona el encoder H.264: hardware (MFT asíncrono, zero-copy GPU) si lo hay, o
// software (CPU) como fallback. El HW MFT se usa en su modo async nativo (eventos
// NEED_INPUT/HAVE_OUTPUT) con un device D3D11 dedicado para no competir con WGC.
// encoder_pref: "Auto"|"NVENC"|"AMF"|"Quick Sync"|"Software"
pub(super) fn build_encoder(
    manager: &IMFDXGIDeviceManager,
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    encoder_pref: &str,
) -> Result<(IMFTransform, Option<IMFMediaEventGenerator>)> {
    let force_sw = std::env::var_os("FLASHBACK_FORCE_SW_ENCODER").is_some()
        || encoder_pref == "Software";

    if !force_sw {
        if let Some(activate) = pick_hw_encoder(encoder_pref)? {
            log::info!(
                "encoder: {} ({width}x{height}, {fps} fps, {} kbps, preferencia {encoder_pref})",
                encoder_name(&activate).unwrap_or_else(|| "hardware sin nombre".into()),
                bitrate / 1000
            );
            let encoder: IMFTransform = unsafe { activate.ActivateObject()? };
            unsafe {
                let attrs = encoder.GetAttributes()?;
                attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
                let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);
                let unk: windows::core::IUnknown = manager.cast()?;
                encoder.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, unk.as_raw() as usize)?;
            }
            configure_encoder_types(&encoder, width, height, fps, bitrate)?;
            let events: IMFMediaEventGenerator = encoder.cast()?;
            return Ok((encoder, Some(events)));
        }
    }

    // Fallback por software: MFT síncrono, sin device manager (codifica en CPU).
    log::info!(
        "encoder: software ({width}x{height}, {fps} fps, {} kbps, preferencia {encoder_pref}{})",
        bitrate / 1000,
        if force_sw { "" } else { ", sin encoder por hardware utilizable" }
    );
    let activate = enum_encoder(MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_TRANSCODE_ONLY)?
        .ok_or_else(|| windows::core::Error::from_hresult(MF_E_TOPO_CODEC_NOT_FOUND))?;
    let encoder: IMFTransform = unsafe { activate.ActivateObject()? };
    configure_encoder_types(&encoder, width, height, fps, bitrate)?;
    Ok((encoder, None))
}

// Encoders H.264 por hardware que se pueden usar, en el orden de calidad de MF, con su nombre.
// Los inservibles se descartan: si solo queda uno así, mejor el software que clips sin imagen.
fn usable_hw_encoders() -> Result<Vec<(IMFActivate, String)>> {
    let info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&info),
            &mut activates,
            &mut count,
        )?;
    }
    if count == 0 || activates.is_null() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for i in 0..count as usize {
        if let Some(act) = unsafe { &*activates.add(i) } {
            let name = encoder_name(act).unwrap_or_default();
            if !crate::capture::unusable_h264_encoder(&name) {
                out.push((act.clone(), name));
            }
        }
    }
    // Liberar el array de activates: drop_in_place decrementa el refcount de cada uno.
    for i in 0..count as usize {
        unsafe { std::ptr::drop_in_place(activates.add(i)) };
    }
    unsafe { CoTaskMemFree(Some(activates as *const _)) };
    Ok(out)
}

// Devuelve el encoder que coincide con la preferencia de vendor. "Auto" devuelve el primero
// (MF ya los ordena por calidad). Si la preferencia no coincide con ningún encoder disponible,
// devuelve el primero como fallback en lugar de fallar.
fn pick_hw_encoder(pref: &str) -> Result<Option<IMFActivate>> {
    let encoders = usable_hw_encoders()?;
    let chosen = encoders
        .iter()
        .find(|(_, name)| vendor_of(name).is_some_and(|v| v.eq_ignore_ascii_case(pref)))
        .or(encoders.first());
    Ok(chosen.map(|(act, _)| act.clone()))
}

const VENDORS: [(&str, &[&str]); 3] =
    [("NVENC", &["nvenc", "nvidia"]), ("AMF", &["amf", "amd"]), ("Quick Sync", &["quick sync", "intel"])];

fn vendor_of(name: &str) -> Option<&'static str> {
    let name = name.to_lowercase();
    VENDORS.iter().find(|(_, keys)| keys.iter().any(|k| name.contains(k))).map(|(v, _)| *v)
}

// Opciones que ofrece Ajustes: los vendors con encoder usable en este equipo y el que elegiría
// "Auto" (Software si no hay ninguno por hardware).
pub(super) fn encoder_options() -> (Vec<&'static str>, &'static str) {
    let encoders = usable_hw_encoders().unwrap_or_default();
    let found: Vec<&str> = encoders.iter().filter_map(|(_, n)| vendor_of(n)).collect();
    let available = VENDORS.iter().map(|(v, _)| *v).filter(|v| found.contains(v)).collect();
    let auto = encoders.first().and_then(|(_, n)| vendor_of(n)).unwrap_or("Software");
    (available, auto)
}

fn encoder_name(activate: &IMFActivate) -> Option<String> {
    let attrs: IMFAttributes = activate.cast().ok()?;
    let mut buf = [0u16; 512];
    let mut len = 0u32;
    unsafe { attrs.GetString(&MFT_FRIENDLY_NAME_Attribute, &mut buf, Some(&mut len)) }.ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

// Primer encoder H.264 que cumple los flags, o None si no hay ninguno.
fn enum_encoder(flags: MFT_ENUM_FLAG) -> Result<Option<IMFActivate>> {
    let info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            flags | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&info),
            &mut activates,
            &mut count,
        )?;
    }
    if count == 0 || activates.is_null() {
        return Ok(None);
    }
    let first = unsafe { (*activates).clone() };
    // Soltar y liberar el array de activates.
    for i in 0..count as usize {
        unsafe {
            let _ = std::ptr::read(activates.add(i));
        }
    }
    unsafe { CoTaskMemFree(Some(activates as *const _)) };
    Ok(first)
}

// Tipos de medio del encoder (output H.264 antes que input NV12, como exigen) + IDR
// periódico. Común a hardware y software.
fn configure_encoder_types(
    encoder: &IMFTransform,
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
) -> Result<()> {
    // El modo de rate control (VBR) debe fijarse ANTES de SetOutputType; si se pone
    // después, el encoder H.264 de MF lo ignora y se queda en CBR. GOP ~1 s (antes 0.5 s):
    // menos IDRs caros = mejor calidad, sin perder mucha granularidad para alinear el
    // guardado del replay al keyframe anterior.
    let gop = fps.max(8);
    let codec = encoder.cast::<ICodecAPI>().ok();
    let quality_failed = codec.as_ref().map(|c| set_quality_codec_settings(c, bitrate, gop));

    let out_type = unsafe { MFCreateMediaType()? };
    unsafe {
        out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        out_type.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
        out_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        // Perfil High (100) + CABAC (lo activa apply_quality_codec_settings). Sin
        // B-frames (DefaultBPictureCount=0): el perfil High sin reordenado conserva el
        // orden salida=entrada, del que depende el emparejado FIFO de timestamps (el que
        // escupe el MFT es poco fiable: a veces sale a 0 y el MP4 queda con duración nula).
        out_type.SetUINT32(&MF_MT_MPEG2_PROFILE, 100)?;
        out_type.SetUINT64(&MF_MT_FRAME_SIZE, pack2(width, height))?;
        out_type.SetUINT64(&MF_MT_FRAME_RATE, pack2(fps, 1))?;
        out_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack2(1, 1))?;
        encoder.SetOutputType(0, &out_type, 0)?;
    }

    let in_type = unsafe { MFCreateMediaType()? };
    unsafe {
        in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        in_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        in_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        in_type.SetUINT64(&MF_MT_FRAME_SIZE, pack2(width, height))?;
        in_type.SetUINT64(&MF_MT_FRAME_RATE, pack2(fps, 1))?;
        in_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack2(1, 1))?;
        encoder.SetInputType(0, &in_type, 0)?;
    }

    if let (Some(codec), Some(failed)) = (&codec, &quality_failed) {
        log_encoder_quality(codec, "replay", bitrate, gop, failed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_never_picks_an_unusable_hardware_encoder() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        }
        for pref in ["Auto", "NVENC", "AMF", "Quick Sync"] {
            let name = pick_hw_encoder(pref).unwrap().and_then(|a| encoder_name(&a));
            assert!(!name.as_deref().is_some_and(crate::capture::unusable_h264_encoder), "{pref}: {name:?}");
        }
    }

    #[test]
    fn encoders_are_grouped_by_the_vendor_names_settings_offers() {
        assert_eq!(vendor_of("NVIDIA H.264 Encoder MFT"), Some("NVENC"));
        assert_eq!(vendor_of("AMDh264Encoder"), Some("AMF"));
        assert_eq!(vendor_of("Intel® Quick Sync Video H.264 Encoder MFT"), Some("Quick Sync"));
        assert_eq!(vendor_of("H264 Encoder MFT"), None);
    }

    // Lo que lista Ajustes sale de la misma enumeración que usa la captura: si hay un encoder por
    // hardware usable, "Auto" apunta a su vendor y ese vendor aparece entre las opciones.
    #[test]
    fn settings_list_the_vendor_auto_would_pick() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let (available, auto) = encoder_options();
        let picked = pick_hw_encoder("Auto").unwrap().and_then(|a| encoder_name(&a)).and_then(|n| vendor_of(&n));
        assert_eq!(auto, picked.unwrap_or("Software"));
        if auto != "Software" {
            assert!(available.contains(&auto));
        }
    }
}
