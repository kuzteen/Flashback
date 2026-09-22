// Formato de salida del export y geometría del encuadre vertical. La geometría es pura y se prueba
// en cualquier plataforma; la composición en GPU vive en el submódulo de Windows.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fill {
    #[default]
    Crop,
    Fit,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Horizontal,
    Vertical { fill: Fill },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const VERTICAL_W: u32 = 1080;

// Un preset de compartir que baja la resolución fija el lado corto: así 720 da 720x1280, con el
// mismo número de píxeles que el 1280x720 horizontal que ya calcula el reparto de bitrate.
pub fn vertical_size(max_short: Option<u32>) -> (u32, u32) {
    let w = max_short.map_or(VERTICAL_W, |m| m.min(VERTICAL_W)) & !1;
    let h = ((w as u64 * 16 / 9) as u32 + 1) & !1;
    (w, h)
}

// El recorte vertical amplía una ventana del origen, así que el bitrate se escala por píxeles de
// salida y no se copia tal cual; los límites evitan un clip inservible o desproporcionado.
pub fn vertical_bitrate(src_bps: u32, src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> u32 {
    let src_px = (src_w as u64 * src_h as u64).max(1);
    let out_px = out_w as u64 * out_h as u64;
    (src_bps as u64 * out_px / src_px).clamp(4_000_000, 40_000_000) as u32
}

pub fn crop_rect(src_w: u32, src_h: u32, crop_x: f64) -> Rect {
    let (sw, sh) = (src_w as f32, src_h as f32);
    let w = (sh * 9.0 / 16.0).min(sw);
    let center = crop_x.clamp(0.0, 1.0) as f32 * sw;
    let x = (center - w / 2.0).clamp(0.0, sw - w);
    Rect { x, y: 0.0, w, h: sh }
}

pub fn fit_rect(src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> Rect {
    let (sw, sh, ow, oh) = (src_w as f32, src_h as f32, out_w as f32, out_h as f32);
    let s = (ow / sw).min(oh / sh);
    let (w, h) = (sw * s, sh * s);
    Rect { x: (ow - w) / 2.0, y: (oh - h) / 2.0, w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn vertical_is_1080_by_1920_and_a_preset_sets_the_short_side() {
        assert_eq!(vertical_size(None), (1080, 1920));
        assert_eq!(vertical_size(Some(720)), (720, 1280));
        assert_eq!(vertical_size(Some(480)), (480, 854));
        assert_eq!(vertical_size(Some(2160)), (1080, 1920));
        assert_eq!(vertical_size(Some(481)), (480, 854));
    }

    #[test]
    fn vertical_bitrate_follows_the_pixel_count_within_bounds() {
        assert_eq!(vertical_bitrate(20_000_000, 1920, 1080, 1080, 1920), 20_000_000);
        assert_eq!(vertical_bitrate(1_000_000, 1920, 1080, 1080, 1920), 4_000_000);
        assert_eq!(vertical_bitrate(100_000_000, 1920, 1080, 1080, 1920), 40_000_000);
    }

    #[test]
    fn the_crop_window_is_full_height_nine_by_sixteen_and_clamped() {
        assert_eq!(crop_rect(1920, 1080, 0.5), r(656.25, 0.0, 607.5, 1080.0));
        assert_eq!(crop_rect(1920, 1080, 0.0), r(0.0, 0.0, 607.5, 1080.0));
        assert_eq!(crop_rect(1920, 1080, 1.0), r(1312.5, 0.0, 607.5, 1080.0));
        assert_eq!(crop_rect(1920, 1080, 7.0), crop_rect(1920, 1080, 1.0));
    }

    #[test]
    fn an_already_vertical_source_is_used_whole() {
        assert_eq!(crop_rect(1080, 1920, 0.3), r(0.0, 0.0, 1080.0, 1920.0));
    }

    #[test]
    fn fit_scales_the_frame_to_the_width_and_centres_it() {
        assert_eq!(fit_rect(1920, 1080, 1080, 1920), r(0.0, 656.25, 1080.0, 607.5));
        assert_eq!(fit_rect(1080, 1920, 1080, 1920), r(0.0, 0.0, 1080.0, 1920.0));
    }

    #[test]
    fn formats_read_and_write_the_frontend_shape() {
        let v: OutputFormat = serde_json::from_str(r#"{"kind":"vertical","fill":"fit"}"#).unwrap();
        assert_eq!(v, OutputFormat::Vertical { fill: Fill::Fit });
        let h: OutputFormat = serde_json::from_str(r#"{"kind":"horizontal"}"#).unwrap();
        assert_eq!(h, OutputFormat::Horizontal);
        assert_eq!(
            serde_json::to_string(&OutputFormat::Vertical { fill: Fill::Crop }).unwrap(),
            r#"{"kind":"vertical","fill":"crop"}"#
        );
    }

    #[test]
    fn an_edit_saved_before_the_format_existed_reads_as_horizontal_and_centred() {
        let e: crate::editor::ClipEdit = serde_json::from_str(
            r#"{"segments":[{"start_ms":0,"end_ms":1000}],"mixer":{"sys_vol":1,"sys_muted":false,"mic_vol":1,"mic_muted":false}}"#,
        )
        .unwrap();
        assert_eq!(e.format, OutputFormat::Horizontal);
        assert_eq!(e.segments[0].crop_x, None);
    }
}
