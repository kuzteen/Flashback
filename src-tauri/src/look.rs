use serde::{Deserialize, Serialize};

// Ajustes de imagen del editor. Solo se materializan al exportar, como el resto de la edición.
// Todos valen 0 en neutro; brillo, contraste, saturación y temperatura van de -1 a 1 y la nitidez
// de 0 a 1. La misma cuenta está en src/lib/look.ts para que la vista previa sea lo exportado.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq)]
#[serde(default)]
pub struct Look {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub sharpness: f32,
}

const EPS: f32 = 1e-4;

impl Look {
    pub fn clamped(self) -> Self {
        let b = |v: f32| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        Self {
            brightness: b(self.brightness),
            contrast: b(self.contrast),
            saturation: b(self.saturation),
            temperature: b(self.temperature),
            sharpness: if self.sharpness.is_finite() { self.sharpness.clamp(0.0, 1.0) } else { 0.0 },
        }
    }

    pub fn is_neutral(&self) -> bool {
        !self.has_color() && self.sharpness.abs() < EPS
    }

    pub fn has_color(&self) -> bool {
        [self.brightness, self.contrast, self.saturation, self.temperature]
            .iter()
            .any(|v| v.abs() >= EPS)
    }

    // Brillo, contraste, saturación y temperatura son lineales sobre los valores sRGB, así que se
    // resuelven en una sola matriz: filas = canales de salida (R, G, B), columnas = entrada R, G, B
    // y desplazamiento. Orden: temperatura, saturación, contraste y brillo.
    pub fn color_matrix(&self) -> [[f32; 4]; 3] {
        const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
        let temp = [1.0 + 0.1 * self.temperature, 1.0, 1.0 - 0.1 * self.temperature];
        let sat = 1.0 + self.saturation;
        let k = 1.0 + self.contrast;
        let offset = (1.0 - k) * 0.5 + 0.2 * self.brightness;
        let mut m = [[0.0f32; 4]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            for j in 0..3 {
                let s = (1.0 - sat) * LUMA[j] + if i == j { sat } else { 0.0 };
                row[j] = k * s * temp[j];
            }
            row[3] = offset;
        }
        m
    }

    // Enfoque por máscara de desenfoque en 3x3: el centro refuerza lo que difiere de sus vecinos.
    pub fn sharpen_kernel(&self) -> Option<[f32; 9]> {
        if self.sharpness < EPS {
            return None;
        }
        let a = 0.6 * self.sharpness;
        Some([0.0, -a, 0.0, -a, 1.0 + 4.0 * a, -a, 0.0, -a, 0.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [[f32; 4]; 3], b: [[f32; 4]; 3]) {
        for i in 0..3 {
            for j in 0..4 {
                assert!((a[i][j] - b[i][j]).abs() < 1e-4, "{a:?} != {b:?}");
            }
        }
    }

    #[test]
    fn neutral_is_the_identity_and_needs_no_kernel() {
        let l = Look::default();
        assert!(l.is_neutral());
        near(l.color_matrix(), [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);
        assert_eq!(l.sharpen_kernel(), None);
    }

    #[test]
    fn brightness_and_contrast_shift_and_scale_around_mid_grey() {
        let b = Look { brightness: 0.5, ..Default::default() };
        near(b.color_matrix(), [[1.0, 0.0, 0.0, 0.1], [0.0, 1.0, 0.0, 0.1], [0.0, 0.0, 1.0, 0.1]]);
        let c = Look { contrast: 0.5, ..Default::default() };
        near(c.color_matrix(), [[1.5, 0.0, 0.0, -0.25], [0.0, 1.5, 0.0, -0.25], [0.0, 0.0, 1.5, -0.25]]);
    }

    #[test]
    fn zero_saturation_turns_every_channel_into_luma() {
        let m = Look { saturation: -1.0, ..Default::default() }.color_matrix();
        near(m, [[0.2126, 0.7152, 0.0722, 0.0], [0.2126, 0.7152, 0.0722, 0.0], [0.2126, 0.7152, 0.0722, 0.0]]);
    }

    #[test]
    fn warm_temperature_lifts_red_and_cuts_blue() {
        let m = Look { temperature: 1.0, ..Default::default() }.color_matrix();
        near(m, [[1.1, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.9, 0.0]]);
    }

    #[test]
    fn sharpness_builds_a_kernel_that_keeps_flat_areas() {
        let k = Look { sharpness: 1.0, ..Default::default() }.sharpen_kernel().unwrap();
        assert!((k.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert!((k[4] - 3.4).abs() < 1e-6);
        assert!(!Look { sharpness: 1.0, ..Default::default() }.is_neutral());
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let l = Look { brightness: 3.0, sharpness: -1.0, contrast: f32::NAN, ..Default::default() }.clamped();
        assert_eq!(l, Look { brightness: 1.0, ..Default::default() });
    }
}
