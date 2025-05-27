use eframe::egui::Color32;

pub const BLACK: Color32 = Color32::BLACK;
pub const WHITE: Color32 = Color32::WHITE;
pub const YELLOW: Color32 = Color32::YELLOW;
pub const DARK_GRAY: Color32 = Color32::DARK_GRAY;
pub const RED: Color32 = Color32::RED;
pub const TRANSPARENT: Color32 = Color32::TRANSPARENT;

pub const GRAY_10: Color32 = Color32::from_gray(10);
pub const GRAY_30: Color32 = Color32::from_gray(30);
pub const GRAY_40: Color32 = Color32::from_gray(40);
pub const GRAY_50: Color32 = Color32::from_gray(50);
pub const GRAY_180: Color32 = Color32::from_gray(180);
pub const GRAY_230: Color32 = Color32::from_gray(230);
pub const GRAY_240: Color32 = Color32::from_gray(240);

pub const VERY_LIGHT_BLUE: Color32 = Color32::from_rgb(220, 230, 245);
pub const VERY_LIGHT_BLUE2: Color32 = Color32::from_rgb(200, 220, 240);
pub const VERY_LIGHT_BLUE3: Color32 = Color32::from_rgb(220, 240, 255);
pub const LIGHT_BLUE: Color32 = Color32::from_rgb(134, 202, 227);
pub const MILD_BLUE: Color32 = Color32::from_rgb(55, 127, 153);
pub const MILD_BLUE2: Color32 = Color32::from_rgb(50, 150, 200);
pub const INTENSE_BLUE: Color32 = Color32::from_rgb(50, 150, 220);
pub const INTENSE_BLUE2: Color32 = Color32::from_rgb(0, 110, 230);
pub const DARK_BLUE: Color32 = Color32::from_rgb(51, 102, 153);

pub const TRANSPARENT_GRAY: Color32 = Color32::from_rgba_premultiplied(30, 30, 30, 230);
pub const BLUE_DARK_GRAY: Color32 = Color32::from_rgb(60, 60, 70);
pub const ALMOST_BLACK: Color32 = Color32::from_rgb(10, 10, 20);
pub const VERY_LIGHT_YELLOW: Color32 = Color32::from_rgb(255, 255, 220);
pub const DARK_YELLOW: Color32 = Color32::from_rgb(242, 176, 34);
pub const MILD_RED: Color32 = Color32::from_rgb(220, 50, 50);
pub const INTENSE_RED: Color32 = Color32::from_rgb(255, 51, 0);

pub fn transparent_yellow() -> Color32 {
    Color32::from_rgba_unmultiplied(242, 176, 34, 1)
}
