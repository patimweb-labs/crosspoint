//! Umrechnung zwischen dB und linearem Pegel.
//!
//! Steht hier und nicht im DSP, weil das Modell sie braucht (Pegel im Zustand,
//! `levels_into`) und jeder Kern sie ohnehin mitbringt. Der Kern nimmt dieselbe
//! Rechnung über `crate::dsp` wieder auf.

/// dBFS → linearer Faktor (0 dB = 1,0).
#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Linearer Faktor → dBFS; unter 1e-7 gilt es als Stille (−140 dBFS).
#[inline]
pub fn lin_to_db(v: f32) -> f32 {
    if v <= 1e-7 {
        -140.0
    } else {
        20.0 * v.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_umrechnung() {
        assert!((db_to_lin(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_lin(-6.0206) - 0.5).abs() < 1e-4);
        assert!((db_to_lin(12.0) - 3.981).abs() < 1e-3);
        // Stille: kein Logarithmus von 0.
        assert_eq!(lin_to_db(0.0), -140.0);
        assert!((lin_to_db(1.0)).abs() < 1e-6);
    }
}
