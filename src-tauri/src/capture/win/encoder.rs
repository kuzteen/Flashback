use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AdapterLuid {
    low: u32,
    high: i32,
}

impl From<LUID> for AdapterLuid {
    fn from(value: LUID) -> Self {
        Self { low: value.LowPart, high: value.HighPart }
    }
}

impl AdapterLuid {
    fn format(self) -> String {
        format!("{:08X}:{:08X}", self.high as u32, self.low)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncoderCandidate {
    name: String,
    adapter: AdapterLuid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EncoderChoice {
    Software,
    Hardware(usize),
}

#[derive(Debug)]
enum EncoderSelectionError {
    Enumeration(windows::core::Error),
    PreferenceUnavailable(String),
}

fn map_hardware_enumeration<T>(
    result: Result<T>,
) -> std::result::Result<T, EncoderSelectionError> {
    result.map_err(EncoderSelectionError::Enumeration)
}

fn select_encoder_candidate(
    pref: &str,
    adapter: AdapterLuid,
    candidates: &[EncoderCandidate],
) -> std::result::Result<EncoderChoice, EncoderSelectionError> {
    if pref == "Software" {
        return Ok(EncoderChoice::Software);
    }

    let matching_adapter = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.adapter == adapter);

    if pref == "Auto" {
        return Ok(matching_adapter
            .map(|(index, _)| index)
            .next()
            .map(EncoderChoice::Hardware)
            .unwrap_or(EncoderChoice::Software));
    }

    let preferred_vendor = vendor_of(pref);
    let selected = matching_adapter
        .clone()
        .find(|(_, candidate)| {
            preferred_vendor
                .is_some_and(|vendor| vendor_of(&candidate.name) == Some(vendor))
        })
        .or_else(|| {
            // Los MFT cuyo nombre no identifica un fabricante siguen siendo seleccionables
            // por su nombre visible exacto.
            preferred_vendor.is_none().then(|| {
                matching_adapter
                    .clone()
                    .find(|(_, candidate)| candidate.name.eq_ignore_ascii_case(pref))
            })
            .flatten()
        });

    selected
        .map(|(index, _)| EncoderChoice::Hardware(index))
        .ok_or_else(|| EncoderSelectionError::PreferenceUnavailable(pref.to_string()))
}

fn encoder_options_for_candidates(
    adapter: AdapterLuid,
    candidates: &[EncoderCandidate],
) -> (Vec<String>, String) {
    let mut available = Vec::new();
    for (vendor, _) in VENDORS {
        if candidates.iter().any(|candidate| {
            candidate.adapter == adapter && vendor_of(&candidate.name) == Some(vendor)
        }) {
            available.push(vendor.to_string());
        }
    }
    for candidate in candidates.iter().filter(|candidate| {
        candidate.adapter == adapter && vendor_of(&candidate.name).is_none()
    }) {
        if !available.iter().any(|name| name == &candidate.name) {
            available.push(candidate.name.clone());
        }
    }

    let auto = match select_encoder_candidate("Auto", adapter, candidates) {
        Ok(EncoderChoice::Hardware(index)) => {
            let name = &candidates[index].name;
            vendor_of(name).unwrap_or(name).to_string()
        }
        Ok(EncoderChoice::Software) => "Software".to_string(),
        Err(_) => unreachable!("Auto no produce una preferencia manual no disponible"),
    };
    (available, auto)
}

fn encoder_diagnostic_once(key: String, message: String, level: log::Level) {
    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let seen = SEEN.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    if !seen.lock().unwrap_or_else(|e| e.into_inner()).insert(key) {
        return;
    }
    match level {
        log::Level::Error => log::error!("{message}"),
        log::Level::Warn => log::warn!("{message}"),
        _ => log::info!("{message}"),
    }
}

fn log_mf_failure(stage: &str, encoder: &str, error: &windows::core::Error) {
    let code = error.code().0 as u32;
    encoder_diagnostic_once(
        format!("mf-error|{encoder}|{stage}|{code:08X}"),
        format!(
            "encoder init: encoder={encoder:?} stage={stage} failed HRESULT=0x{code:08X} ({error})"
        ),
        log::Level::Error,
    );
}

fn log_mf_result<T>(stage: &str, encoder: &str, result: Result<T>) -> Result<T> {
    match result {
        Ok(value) => {
            encoder_diagnostic_once(
                format!("mf-ok|{encoder}|{stage}"),
                format!("encoder init: encoder={encoder:?} stage={stage} ok"),
                log::Level::Info,
            );
            Ok(value)
        }
        Err(error) => {
            log_mf_failure(stage, encoder, &error);
            Err(error)
        }
    }
}

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
    adapter_luid: LUID,
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    encoder_pref: &str,
) -> Result<(IMFTransform, Option<IMFMediaEventGenerator>, String)> {
    let force_sw = std::env::var_os("FLASHBACK_FORCE_SW_ENCODER").is_some()
        || encoder_pref == "Software";

    if !force_sw {
        let activate = match pick_hw_encoder(encoder_pref, adapter_luid) {
            Ok(activate) => activate,
            Err(EncoderSelectionError::Enumeration(error)) => return Err(error),
            Err(EncoderSelectionError::PreferenceUnavailable(pref)) => {
                let adapter = AdapterLuid::from(adapter_luid).format();
                encoder_diagnostic_once(
                    format!("unavailable|{pref}|{adapter}"),
                    format!("encoder selection: preference={pref:?} unavailable for capture adapter LUID={adapter}"),
                    log::Level::Error,
                );
                return Err(windows::core::Error::from_hresult(MF_E_TOPO_CODEC_NOT_FOUND));
            }
        };
        if let Some(activate) = activate {
            let name = encoder_name(&activate).unwrap_or_else(|| "hardware sin nombre".into());
            encoder_diagnostic_once(
                format!("init|hardware|{name}|{width}|{height}|{fps}|{bitrate}|{encoder_pref}"),
                format!("encoder: {name} ({width}x{height}, {fps} fps, {} kbps, preferencia {encoder_pref})", bitrate / 1000),
                log::Level::Info,
            );
            let initialized: Result<(IMFTransform, Option<IMFMediaEventGenerator>, String)> = (|| {
                let encoder: IMFTransform = log_mf_result(
                    "ActivateObject",
                    &name,
                    unsafe { activate.ActivateObject() },
                )?;
                unsafe {
                    let attrs = log_mf_result("GetAttributes", &name, encoder.GetAttributes())?;
                    log_mf_result(
                        "Set MF_TRANSFORM_ASYNC_UNLOCK",
                        &name,
                        attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1),
                    )?;
                    if let Err(error) = attrs.SetUINT32(&MF_LOW_LATENCY, 1) {
                        log_mf_failure("Set MF_LOW_LATENCY (opcional)", &name, &error);
                    }
                    let unk: windows::core::IUnknown = log_mf_result(
                        "cast device manager to IUnknown",
                        &name,
                        manager.cast(),
                    )?;
                    log_mf_result(
                        "ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER)",
                        &name,
                        encoder.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, unk.as_raw() as usize),
                    )?;
                }
                configure_encoder_types(&encoder, &name, width, height, fps, bitrate)?;
                let events: IMFMediaEventGenerator = log_mf_result(
                    "cast IMFMediaEventGenerator",
                    &name,
                    encoder.cast(),
                )?;
                start_encoder_streaming(&encoder, &name, true)?;
                Ok((encoder, Some(events), name.clone()))
            })();
            if let Err(error) = &initialized {
                // La activación conserva el transform: soltar el resultado no basta para
                // liberar sus recursos antes de crear el encoder de software.
                let _ = unsafe { activate.ShutdownObject() };
                if encoder_pref == "Auto" {
                    let code = error.code().0 as u32;
                    encoder_diagnostic_once(
                        format!("fallback|{name}|{code:08X}"),
                        format!("encoder: Auto usa software tras fallo de inicialización de {name:?} HRESULT=0x{code:08X}"),
                        log::Level::Warn,
                    );
                }
            }
            return finish_encoder_initialization(encoder_pref, initialized, || {
                build_software_encoder(width, height, fps, bitrate, encoder_pref, "hardware_init_failed")
            });
        }
    }

    build_software_encoder(
        width, height, fps, bitrate, encoder_pref,
        if force_sw { "software_forced" } else { "no_usable_hardware" },
    )
}

// Auto puede recuperarse de un MFT que se enumera pero rechaza la inicialización.
// Una elección manual conserva su error para que el usuario decida cambiarla.
fn finish_encoder_initialization<T>(
    pref: &str,
    hardware: Result<T>,
    software: impl FnOnce() -> Result<T>,
) -> Result<T> {
    match hardware {
        Err(_) if pref == "Auto" => software(),
        result => result,
    }
}

fn start_encoder_streaming(encoder: &IMFTransform, name: &str, asynchronous: bool) -> Result<()> {
    log_mf_result(
        "ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING)", name,
        unsafe { encoder.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0) },
    )?;
    if asynchronous {
        log_mf_result(
            "ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM)", name,
            unsafe { encoder.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0) },
        )?;
    }
    Ok(())
}

fn build_software_encoder(
    width: u32,
    height: u32,
    fps: u32,
    bitrate: u32,
    encoder_pref: &str,
    reason: &str,
) -> Result<(IMFTransform, Option<IMFMediaEventGenerator>, String)> {
    // Fallback por software: MFT síncrono, sin device manager (codifica en CPU).
    encoder_diagnostic_once(
        format!("init|software|{width}|{height}|{fps}|{bitrate}|{encoder_pref}|{reason}"),
        format!("encoder: software ({width}x{height}, {fps} fps, {} kbps, preferencia {encoder_pref}, {reason})", bitrate / 1000),
        log::Level::Info,
    );
    let activate = enum_encoder(MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_TRANSCODE_ONLY)?
        .ok_or_else(|| windows::core::Error::from_hresult(MF_E_TOPO_CODEC_NOT_FOUND))?;
    let name = encoder_name(&activate).unwrap_or_else(|| "software sin nombre".into());
    encoder_diagnostic_once(
        format!("selection|software|{name}|{reason}|{encoder_pref}"),
        format!(
            "encoder selection: preference={encoder_pref:?} selected={name:?} reason={reason} path=software"
        ),
        log::Level::Info,
    );
    let encoder: IMFTransform = log_mf_result(
        "ActivateObject (software)",
        &name,
        unsafe { activate.ActivateObject() },
    )?;
    configure_encoder_types(&encoder, &name, width, height, fps, bitrate)?;
    start_encoder_streaming(&encoder, &name, false)?;
    Ok((encoder, None, name))
}

// Encoders H.264 de hardware asociados al adaptador; se conserva el orden de Media Foundation.
// Los inservibles se descartan: si solo queda uno así, mejor el software que clips sin imagen.
fn usable_hw_encoders(adapter_luid: LUID) -> Result<Vec<(IMFActivate, String)>> {
    let info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    let adapter = AdapterLuid::from(adapter_luid).format();
    let mut attributes = None;
    log_mf_result(
        "MFCreateAttributes (adapter-scoped MFT enumeration)",
        &format!("adapter LUID={adapter}"),
        unsafe { MFCreateAttributes(&mut attributes, 1) },
    )?;
    let attributes = attributes.ok_or_else(null_out)?;
    let luid_bytes = unsafe {
        std::slice::from_raw_parts(
            (&adapter_luid as *const LUID).cast::<u8>(),
            std::mem::size_of::<LUID>(),
        )
    };
    log_mf_result(
        "SetBlob(MFT_ENUM_ADAPTER_LUID)",
        &format!("adapter LUID={adapter}"),
        unsafe { attributes.SetBlob(&MFT_ENUM_ADAPTER_LUID, luid_bytes) },
    )?;
    let enumeration = unsafe {
        MFTEnum2(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&info),
            &attributes,
            &mut activates,
            &mut count,
        )
    };
    let activate_array = ActivateArray { ptr: activates, count };
    if let Err(error) = enumeration {
        log_mf_failure(
            "MFTEnum2 (hardware H.264, adapter-scoped)",
            &format!("adapter LUID={adapter}"),
            &error,
        );
        return Err(error);
    }
    if count == 0 {
        encoder_diagnostic_once(
            format!("enumeration|hardware|{adapter}|empty"),
            format!("encoder enumeration: hardware H.264 adapter LUID={adapter} candidates=[]"),
            log::Level::Info,
        );
        return Ok(Vec::new());
    }
    if activate_array.ptr.is_null() {
        let error = windows::core::Error::from_hresult(E_POINTER);
        log_mf_failure(
            "MFTEnum2 returned a nonzero count with a null activation array",
            &format!("adapter LUID={adapter}"),
            &error,
        );
        return Err(error);
    }
    let mut out = Vec::new();
    let mut report = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        if let Some(act) = unsafe { &*activate_array.ptr.add(i) } {
            let name = encoder_name(act)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("MFT H.264 sin nombre [{i}]"));
            let vendor = vendor_of(&name).unwrap_or("desconocido");
            if crate::capture::unusable_h264_encoder(&name) {
                report.push(format!("[{i}] {name:?} vendor={vendor} filtrado=DX12"));
            } else {
                report.push(format!("[{i}] {name:?} vendor={vendor} usable"));
                out.push((act.clone(), name));
            }
        } else {
            report.push(format!("[{i}] <null> filtrado=activación nula"));
        }
    }
    let summary = report.join("; ");
    encoder_diagnostic_once(
        format!("enumeration|hardware|{adapter}|{summary}"),
        format!("encoder enumeration: hardware H.264 candidates for adapter LUID={adapter} in MFT order: [{summary}]"),
        log::Level::Info,
    );
    Ok(out)
}

struct ActivateArray {
    ptr: *mut Option<IMFActivate>,
    count: u32,
}

impl Drop for ActivateArray {
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        for index in 0..self.count as usize {
            unsafe { std::ptr::drop_in_place(self.ptr.add(index)) };
        }
        unsafe { CoTaskMemFree(Some(self.ptr as *const _)) };
    }
}

// Auto conserva el primer candidato asociado al adaptador. Una preferencia manual solo
// selecciona candidatos de ese adaptador y falla explícitamente si no encuentra coincidencia.
fn pick_hw_encoder(
    pref: &str,
    adapter_luid: LUID,
) -> std::result::Result<Option<IMFActivate>, EncoderSelectionError> {
    let adapter = AdapterLuid::from(adapter_luid);
    let encoders = map_hardware_enumeration(usable_hw_encoders(adapter_luid))?;
    let candidates: Vec<_> = encoders
        .iter()
        .map(|(_, name)| EncoderCandidate { name: name.clone(), adapter })
        .collect();
    let choice = select_encoder_candidate(pref, adapter, &candidates)?;
    if let EncoderChoice::Hardware(index) = choice {
        let (activate, name) = &encoders[index];
        let reason = if pref == "Auto" {
            "first_adapter_scoped_MFT_candidate_for_Auto"
        } else if vendor_of(pref).is_some() {
            "manufacturer_match"
        } else {
            "exact_friendly_name_match"
        };
        encoder_diagnostic_once(
            format!("selection|{}|{pref}|{index}|{name}|{reason}", adapter.format()),
            format!(
                "encoder selection: preference={pref:?} adapter_luid={} candidate_index={index} selected={name:?} vendor={} reason={reason}",
                adapter.format(), vendor_of(name).unwrap_or("desconocido")
            ),
            log::Level::Info,
        );
        Ok(Some(activate.clone()))
    } else {
        encoder_diagnostic_once(
            format!("selection|{}|{pref}|software", adapter.format()),
            format!("encoder selection: preference={pref:?} adapter_luid={} selected=software reason=no_adapter_scoped_hardware_candidate", adapter.format()),
            log::Level::Info,
        );
        Ok(None)
    }
}

const VENDORS: [(&str, &[&str]); 3] =
    [("NVENC", &["nvenc", "nvidia"]), ("AMF", &["amf", "amd"]), ("Quick Sync", &["quick sync", "intel"])];

fn vendor_of(name: &str) -> Option<&'static str> {
    let name = name.to_lowercase();
    VENDORS.iter().find(|(_, keys)| keys.iter().any(|k| name.contains(k))).map(|(v, _)| *v)
}

// Opciones que ofrece Ajustes: los vendors con encoder usable en este equipo y el que elegiría
// "Auto" (Software si no hay ninguno por hardware).
pub(super) fn encoder_options(adapter_luid: LUID) -> Result<(Vec<String>, String)> {
    let encoders = usable_hw_encoders(adapter_luid)?;
    let adapter = AdapterLuid::from(adapter_luid);
    let candidates: Vec<_> = encoders
        .iter()
        .map(|(_, name)| EncoderCandidate { name: name.clone(), adapter })
        .collect();
    let (available, auto) = encoder_options_for_candidates(adapter, &candidates);
    Ok((available, auto))
}

fn encoder_name(activate: &IMFActivate) -> Option<String> {
    let attrs: IMFAttributes = match activate.cast() {
        Ok(attrs) => attrs,
        Err(error) => {
            log_mf_failure("read friendly name attributes", "MFT candidate", &error);
            return None;
        }
    };
    let mut buf = [0u16; 512];
    let mut len = 0u32;
    if let Err(error) = unsafe {
        attrs.GetString(&MFT_FRIENDLY_NAME_Attribute, &mut buf, Some(&mut len))
    } {
        log_mf_failure("GetString(MFT_FRIENDLY_NAME_Attribute)", "MFT candidate", &error);
        return None;
    }
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
    let enumeration = unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            flags | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&info),
            &mut activates,
            &mut count,
        )
    };
    if let Err(error) = enumeration {
        log_mf_failure("MFTEnumEx (software H.264)", "Media Foundation", &error);
        return Err(error);
    }
    if count == 0 || activates.is_null() {
        encoder_diagnostic_once(
            "enumeration|software|empty".into(),
            "encoder enumeration: synchronous H.264 candidates=[]".into(),
            log::Level::Info,
        );
        return Ok(None);
    }
    let first = unsafe { (*activates).clone() };
    let first_name = first
        .as_ref()
        .and_then(encoder_name)
        .unwrap_or_else(|| "software sin nombre".into());
    encoder_diagnostic_once(
        format!("enumeration|software|{count}|{first_name}"),
        format!("encoder enumeration: synchronous H.264 candidates={count}; first={first_name:?}"),
        log::Level::Info,
    );
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
    encoder_name: &str,
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

    let out_type = log_mf_result(
        "MFCreateMediaType (H.264 output)",
        encoder_name,
        unsafe { MFCreateMediaType() },
    )?;
    let output_attrs = unsafe {
        (|| -> Result<()> {
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
            Ok(())
        })()
    };
    log_mf_result("populate H.264 output media type", encoder_name, output_attrs)?;
    log_mf_result(
        "SetOutputType(H.264)",
        encoder_name,
        unsafe { encoder.SetOutputType(0, &out_type, 0) },
    )?;

    let in_type = log_mf_result(
        "MFCreateMediaType (NV12 input)",
        encoder_name,
        unsafe { MFCreateMediaType() },
    )?;
    let input_attrs = unsafe {
        (|| -> Result<()> {
            in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            in_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            in_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            in_type.SetUINT64(&MF_MT_FRAME_SIZE, pack2(width, height))?;
            in_type.SetUINT64(&MF_MT_FRAME_RATE, pack2(fps, 1))?;
            in_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack2(1, 1))?;
            Ok(())
        })()
    };
    log_mf_result("populate NV12 input media type", encoder_name, input_attrs)?;
    log_mf_result(
        "SetInputType(NV12)",
        encoder_name,
        unsafe { encoder.SetInputType(0, &in_type, 0) },
    )?;

    if let (Some(codec), Some(failed)) = (&codec, &quality_failed) {
        log_encoder_quality(codec, "replay", bitrate, gop, failed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(low: u32) -> AdapterLuid {
        AdapterLuid { low, high: 0 }
    }

    fn candidate(name: &str, adapter: AdapterLuid) -> EncoderCandidate {
        EncoderCandidate { name: name.to_string(), adapter }
    }

    #[test]
    fn auto_uses_first_candidate_for_requested_adapter_only() {
        let target = adapter(2);
        let candidates = [
            candidate("NVIDIA H.264 Encoder", adapter(1)),
            candidate("AMDh264Encoder", target),
            candidate("Intel Quick Sync H.264", target),
        ];
        assert_eq!(
            select_encoder_candidate("Auto", target, &candidates).unwrap(),
            EncoderChoice::Hardware(1)
        );
    }

    #[test]
    fn auto_recovers_from_hardware_initialization_failure() {
        let hardware = Err(windows::core::Error::from_hresult(
            windows::core::HRESULT(0x8876086Cu32 as i32),
        ));
        assert_eq!(finish_encoder_initialization("Auto", hardware, || Ok("software")).unwrap(), "software");
    }

    #[test]
    fn working_hardware_does_not_initialize_software() {
        let result = finish_encoder_initialization("Auto", Ok("hardware"), || {
            panic!("el hardware inicializado debe conservarse")
        });
        assert_eq!(result.unwrap(), "hardware");
    }

    #[test]
    fn explicit_hardware_failure_does_not_switch_encoders() {
        for preference in ["NVENC", "AMF", "Quick Sync", "Vendor Z"] {
            let result: Result<()> = finish_encoder_initialization(
                preference,
                Err(windows::core::Error::from_hresult(MF_E_INVALIDMEDIATYPE)),
                || panic!("una preferencia manual debe conservar su error"),
            );
            assert_eq!(result.unwrap_err().code(), MF_E_INVALIDMEDIATYPE);
        }
    }

    #[test]
    fn software_failure_is_reported_after_hardware_failure() {
        let result: Result<()> = finish_encoder_initialization(
            "Auto",
            Err(windows::core::Error::from_hresult(MF_E_INVALIDMEDIATYPE)),
            || Err(windows::core::Error::from_hresult(MF_E_TOPO_CODEC_NOT_FOUND)),
        );
        assert_eq!(result.unwrap_err().code(), MF_E_TOPO_CODEC_NOT_FOUND);
    }

    #[test]
    fn auto_and_software_encoders_start_on_the_capture_device() {
        // Ejercita MFTEnum2, el device manager y los mensajes de streaming reales.
        std::thread::spawn(|| {
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok().unwrap();
            super::super::ensure_mf();
            {
                let (device, _winrt_device) = super::super::create_device().unwrap();
                let luid = super::super::adapter_luid_for_device(&device).unwrap();
                let mut token = 0;
                let mut manager = None;
                unsafe { MFCreateDXGIDeviceManager(&mut token, &mut manager) }.unwrap();
                let manager = manager.unwrap();
                unsafe { manager.ResetDevice(&device, token) }.unwrap();
                for preference in ["Auto", "Software"] {
                    let (encoder, events, name) =
                        build_encoder(&manager, luid, 1920, 1080, 60, 7_464_000, preference)
                            .unwrap();
                    assert!(!name.is_empty());
                    if preference == "Software" {
                        assert!(events.is_none());
                    }
                    unsafe { encoder.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0) }.unwrap();
                }
            }
            unsafe { CoUninitialize() };
        }).join().unwrap();
    }

    #[test]
    fn auto_preserves_media_foundation_order_within_adapter() {
        let target = adapter(4);
        let candidates = [
            candidate("other adapter", adapter(3)),
            candidate("first same adapter", target),
            candidate("second same adapter", target),
        ];
        assert_eq!(
            select_encoder_candidate("Auto", target, &candidates).unwrap(),
            EncoderChoice::Hardware(1)
        );
    }

    #[test]
    fn manual_vendor_preferences_select_only_matching_candidates() {
        let target = adapter(5);
        let candidates = [
            candidate("NVIDIA H.264 Encoder MFT", target),
            candidate("AMDh264Encoder", target),
            candidate("Intel Quick Sync Video H.264 Encoder MFT", target),
        ];
        for (preference, expected) in [("NVENC", 0), ("AMF", 1), ("Quick Sync", 2)] {
            assert_eq!(
                select_encoder_candidate(preference, target, &candidates).unwrap(),
                EncoderChoice::Hardware(expected),
                "{preference}"
            );
        }
    }

    #[test]
    fn unavailable_manual_vendor_is_an_explicit_error() {
        let target = adapter(6);
        let candidates = [
            candidate("NVIDIA H.264 Encoder on another adapter", adapter(60)),
            candidate("AMDh264Encoder", target),
        ];
        assert!(matches!(
            select_encoder_candidate("NVENC", target, &candidates),
            Err(EncoderSelectionError::PreferenceUnavailable(pref)) if pref == "NVENC"
        ));
    }

    #[test]
    fn unknown_manufacturer_name_stays_visible_and_can_be_selected_exactly() {
        let target = adapter(7);
        let candidates = [candidate("Vendor Z H.264 Encoder", target)];
        let (available, auto) = encoder_options_for_candidates(target, &candidates);
        assert_eq!(available, ["Vendor Z H.264 Encoder"]);
        assert_eq!(auto, "Vendor Z H.264 Encoder");
        assert_eq!(
            select_encoder_candidate("Vendor Z H.264 Encoder", target, &candidates).unwrap(),
            EncoderChoice::Hardware(0)
        );
    }

    #[test]
    fn empty_adapter_list_resolves_auto_to_software() {
        let target = adapter(8);
        assert_eq!(select_encoder_candidate("Auto", target, &[]).unwrap(), EncoderChoice::Software);
        assert_eq!(encoder_options_for_candidates(target, &[]), (Vec::new(), "Software".into()));
    }

    #[test]
    fn enumeration_hresult_is_not_converted_to_an_empty_list() {
        let error = windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL);
        let mapped = map_hardware_enumeration::<Vec<EncoderCandidate>>(Err(error));
        assert!(matches!(
            mapped,
            Err(EncoderSelectionError::Enumeration(error))
                if error.code() == windows::Win32::Foundation::E_FAIL
        ));
    }

    #[test]
    fn software_preference_always_resolves_to_software() {
        let target = adapter(9);
        let candidates = [candidate("NVIDIA H.264 Encoder", target)];
        assert_eq!(select_encoder_candidate("Software", target, &candidates).unwrap(), EncoderChoice::Software);
    }

    #[test]
    fn settings_auto_label_matches_the_resolved_candidate() {
        let target = adapter(10);
        let candidates = [
            candidate("other adapter NVIDIA", adapter(11)),
            candidate("Intel Quick Sync H.264 Encoder", target),
        ];
        let choice = select_encoder_candidate("Auto", target, &candidates).unwrap();
        let (available, auto) = encoder_options_for_candidates(target, &candidates);
        let EncoderChoice::Hardware(index) = choice else { panic!("expected hardware candidate") };
        assert_eq!(auto, vendor_of(&candidates[index].name).unwrap());
        assert!(available.contains(&auto));
    }

    #[test]
    fn capture_never_picks_an_unusable_hardware_encoder() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        }
        let adapter_luid = super::super::default_capture_adapter_luid().unwrap();
        let name = pick_hw_encoder("Auto", adapter_luid)
            .unwrap()
            .and_then(|a| encoder_name(&a));
        assert!(!name.as_deref().is_some_and(crate::capture::unusable_h264_encoder), "Auto: {name:?}");
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
        let adapter_luid = super::super::default_capture_adapter_luid().unwrap();
        let (available, auto) = encoder_options(adapter_luid).unwrap();
        let picked = pick_hw_encoder("Auto", adapter_luid)
            .unwrap()
            .and_then(|a| encoder_name(&a))
            .map(|name| vendor_of(&name).unwrap_or(&name).to_string())
            .unwrap_or_else(|| "Software".into());
        assert_eq!(auto, picked);
        if auto != "Software" {
            assert!(available.contains(&auto));
        }
    }
}
