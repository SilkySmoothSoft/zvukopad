//! Шрифты, цвета, иконка программы.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};

pub const ACCENT: Color32 = Color32::from_rgb(242, 153, 74);
pub const PLAYING: Color32 = Color32::from_rgb(120, 200, 120);
pub const DANGER: Color32 = Color32::from_rgb(230, 90, 80);
pub const MUTED: Color32 = Color32::from_rgb(150, 150, 160);

pub fn setup(ctx: &egui::Context) {
    // Segoe UI — родной шрифт Windows, с кириллицей. Если его нет, остаётся встроенный.
    // Segoe UI Symbol / Emoji — запасные шрифты для значков (▶ 🔊 🎧 и т. п.).
    let mut fonts = FontDefinitions::default();
    let main = [r"C:\Windows\Fonts\segoeui.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"];
    let fallbacks = [
        r"C:\Windows\Fonts\seguisym.ttf",
        r"C:\Windows\Fonts\seguiemj.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    ];
    if let Some(bytes) = main.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("system-ui".into(), std::sync::Arc::new(FontData::from_owned(bytes)));
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "system-ui".into());
        }
    }
    for (i, path) in fallbacks.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("symbols-{i}");
            fonts.font_data.insert(name.clone(), std::sync::Arc::new(FontData::from_owned(bytes)));
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                if let Some(list) = fonts.families.get_mut(&family) {
                    list.push(name.clone());
                }
            }
        }
    }
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.visuals.selection.bg_fill = ACCENT.linear_multiply(0.55);
        style.visuals.hyperlink_color = ACCENT;
        for (ts, font) in style.text_styles.iter_mut() {
            font.size = match ts {
                egui::TextStyle::Small => 12.0,
                _ => font.size * 1.08,
            };
        }
    });
}

/// Иконка окна (заголовок, панель задач): маскот «Рыжик» из res/icon-256.png.
pub fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../res/icon-256.png")).unwrap_or_else(|_| fallback_icon())
}

/// Запасная нарисованная иконка (если картинка вдруг не прочиталась).
fn fallback_icon() -> egui::IconData {
    const N: usize = 64;
    let mut rgba = vec![0u8; N * N * 4];
    let bars = [(10, 26), (20, 14), (30, 6), (40, 18), (50, 30)];
    for y in 0..N {
        for x in 0..N {
            let i = (y * N + x) * 4;
            // Скруглённый фон.
            let (cx, cy) = ((x as f32 - 31.5).abs(), (y as f32 - 31.5).abs());
            let r = 12.0;
            let outside =
                cx > 32.0 - r && cy > 32.0 - r && ((cx - (32.0 - r)).powi(2) + (cy - (32.0 - r)).powi(2)).sqrt() > r;
            if outside {
                continue;
            }
            let mut c = [32u8, 34, 44, 255];
            for &(bx, top) in &bars {
                if x >= bx && x < bx + 6 && y >= top && y < N - 10 {
                    c = [242, 153, 74, 255];
                }
            }
            rgba[i..i + 4].copy_from_slice(&c);
        }
    }
    egui::IconData { rgba, width: N as u32, height: N as u32 }
}
