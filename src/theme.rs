pub type Rgb = (u8, u8, u8);

macro_rules! define_palette {
    ($($rgb_name:ident, $ansi_name:ident = ($r:literal, $g:literal, $b:literal);)+) => {
        $(
            pub const $rgb_name: Rgb = ($r, $g, $b);
            pub const $ansi_name: &str = concat!("\x1b[38;2;", stringify!($r), ";", stringify!($g), ";", stringify!($b), "m");
        )+
    };
}

define_palette! {
    BASE_RGB,     _BASE_ANSI     = (30, 30, 46);
    TEXT_RGB,     TEXT           = (205, 214, 244);
    SUBTEXT_RGB,  SUB            = (166, 173, 200);
    SURFACE1_RGB, _SURFACE1_ANSI = (69, 71, 90);
    _DIM_RGB,     DIM            = (108, 112, 134);
    CYAN_RGB,     CYAN           = (0, 229, 255);
    SAPPHIRE_RGB, BLUE           = (56, 189, 248);
    MINT_RGB,     GREEN          = (80, 250, 123);
    _YELLOW_RGB,  YELLOW         = (249, 226, 175);
    _RED_RGB,     RED            = (243, 139, 168);
}

pub const BOLD: &str = "\x1b[1m";
pub const RESET: &str = "\x1b[0m";

/// Gradient stops for the sweeping progress bar in the flutterff-rs theme
pub const FLUTTERFF_RAMP: &[Rgb] = &[
    (2, 125, 253),   // #027DFD Flutter Blue
    (0, 180, 216),   // #00B4D8 Deep Cyan
    (0, 229, 255),   // #00E5FF Electric Cyan
    (114, 239, 221), // #72EFDD Aqua Mint
    (255, 255, 255), // #FFFFFF Ice Highlight
    (114, 239, 221), // #72EFDD Aqua Mint
    (0, 229, 255),   // #00E5FF Electric Cyan
    (2, 125, 253),   // #027DFD Flutter Blue
];

pub fn fg_rgb((r, g, b): Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Blends `fg` toward `bg` by `alpha` (`0.0` = bg, `1.0` = fg), matching cargo-pretty's `fade()`
pub fn blend(fg: Rgb, bg: Rgb, alpha: f32) -> Rgb {
    let a = alpha.clamp(0.0, 1.0);
    let lerp = |f: u8, b: u8| (b as f32 + (f as f32 - b as f32) * a).round() as u8;
    (lerp(fg.0, bg.0), lerp(fg.1, bg.1), lerp(fg.2, bg.2))
}

pub fn sample_flutterff_ramp(t: f32) -> Rgb {
    let u = t.rem_euclid(1.0);
    let max_idx = FLUTTERFF_RAMP.len() - 1;
    let pos = u * max_idx as f32;
    let idx = pos.floor() as usize;
    let frac = pos - idx as f32;
    blend(
        FLUTTERFF_RAMP[(idx + 1).min(max_idx)],
        FLUTTERFF_RAMP[idx],
        frac,
    )
}

pub fn now_timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

pub fn print_tagged_event(tag: &str, tag_color: &str, message: &str) {
    println!(
        "{}{}{} {}{}{} {}{}{}",
        DIM,
        now_timestamp(),
        RESET,
        tag_color,
        tag,
        RESET,
        TEXT,
        message,
        RESET
    );
}
