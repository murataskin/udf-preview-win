//! ARGB colour conversion, mirroring `ColorConversion.swift`.
//!
//! Colours are stored in the UDF as a **signed** 32-bit ARGB integer (`i32`). The high
//! byte is alpha, then R, G, B. Reinterpret the bits as `u32` before shifting (a negative
//! `i32` like red `0xFFFF0000` must not sign-extend).

/// Convert a signed ARGB `i32` to `(r, g, b, a)` byte components.
pub fn argb_to_rgba(value: i32) -> (u8, u8, u8, u8) {
    let unsigned = value as u32; // bit-pattern reinterpret, matches Swift UInt32(bitPattern:)
    let a = ((unsigned >> 24) & 0xFF) as u8;
    let r = ((unsigned >> 16) & 0xFF) as u8;
    let g = ((unsigned >> 8) & 0xFF) as u8;
    let b = (unsigned & 0xFF) as u8;
    (r, g, b, a)
}

/// CSS `#rrggbb` for a fill/text colour (alpha dropped).
pub fn css_hex(value: i32) -> String {
    let (r, g, b, _) = argb_to_rgba(value);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// CSS `rgba(r, g, b, a)` honouring the stored alpha (0.0–1.0).
pub fn css_rgba(value: i32) -> String {
    let (r, g, b, a) = argb_to_rgba(value);
    format!("rgba({r}, {g}, {b}, {:.3})", a as f64 / 255.0)
}

/// Border colour as an **opaque** `(r, g, b, 255)`.
///
/// Border visibility is driven by `borderStyle`/`borderSpec`, **not** the colour's alpha.
/// The official editor (and the reference's `configureBorders`) ignore the alpha channel
/// for borders and force full opacity. The headline consequence: `borderColor = 0` — the
/// common case — is **opaque black**, not transparent. Do not feed a border colour through
/// the alpha-aware paths above.
pub fn border_rgb_opaque(value: i32) -> (u8, u8, u8, u8) {
    let (r, g, b, _alpha_ignored) = argb_to_rgba(value);
    (r, g, b, 255)
}

/// CSS `#rrggbb` for a border colour (always opaque; see [`border_rgb_opaque`]).
pub fn border_css_hex(value: i32) -> String {
    let (r, g, b, _) = border_rgb_opaque(value);
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_red_from_negative_i32() {
        // 0xFFFF0000 as i32 is negative; must not sign-extend.
        let red = 0xFFFF0000u32 as i32;
        assert_eq!(argb_to_rgba(red), (255, 0, 0, 255));
        assert_eq!(css_hex(red), "#ff0000");
    }

    #[test]
    fn opaque_black_standard() {
        let black = 0xFF000000u32 as i32; // -16777216
        assert_eq!(argb_to_rgba(black), (0, 0, 0, 255));
    }

    #[test]
    fn fully_transparent_zero() {
        // As a *fill/text* colour, 0 is fully transparent (alpha byte = 0).
        assert_eq!(argb_to_rgba(0), (0, 0, 0, 0));
        assert_eq!(css_rgba(0), "rgba(0, 0, 0, 0.000)");
    }

    #[test]
    fn border_color_zero_is_opaque_black() {
        // The critical rule: as a *border* colour, 0 must render opaque black.
        assert_eq!(border_rgb_opaque(0), (0, 0, 0, 255));
        assert_eq!(border_css_hex(0), "#000000");
    }

    #[test]
    fn border_ignores_alpha() {
        // A semi-transparent blue border still renders fully opaque.
        let semi_blue = 0x800000FFu32 as i32; // alpha 0x80, blue 0xFF
        assert_eq!(border_rgb_opaque(semi_blue), (0, 0, 255, 255));
    }

    #[test]
    fn channel_order_rgb() {
        let c = 0x00112233; // a=00 r=11 g=22 b=33
        assert_eq!(argb_to_rgba(c), (0x11, 0x22, 0x33, 0x00));
        assert_eq!(css_hex(c), "#112233");
    }
}
