//! Catppuccin palettes for the aski popup.
//!
//! Colors are the official Catppuccin hex values, see https://github.com/catppuccin/catppuccin

use crate::spec::Flavor;
use iced::Color;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub crust: Color,
    pub mantle: Color,
    pub base: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub surface2: Color,
    pub overlay0: Color,
    pub overlay1: Color,
    pub subtext0: Color,
    pub text: Color,
    pub accent: Color,
}

const fn c(rgb: u32) -> Color {
    Color {
        r: ((rgb >> 16) & 0xff) as f32 / 255.0,
        g: ((rgb >> 8) & 0xff) as f32 / 255.0,
        b: (rgb & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// Core surface/text colors per flavor.
struct Core {
    crust: u32,
    mantle: u32,
    base: u32,
    surface0: u32,
    surface1: u32,
    surface2: u32,
    overlay0: u32,
    overlay1: u32,
    subtext0: u32,
    text: u32,
}

const MOCHA: Core = Core {
    crust: 0x11111b,
    mantle: 0x181825,
    base: 0x1e1e2e,
    surface0: 0x313244,
    surface1: 0x45475a,
    surface2: 0x585b70,
    overlay0: 0x6c7086,
    overlay1: 0x7f849c,
    subtext0: 0xa6adc8,
    text: 0xcdd6f4,
};

const MACCHIATO: Core = Core {
    crust: 0x181926,
    mantle: 0x1e2030,
    base: 0x24273a,
    surface0: 0x363a4f,
    surface1: 0x494d64,
    surface2: 0x5b6078,
    overlay0: 0x6e738d,
    overlay1: 0x8087a2,
    subtext0: 0xa5adcb,
    text: 0xcad3f5,
};

const FRAPPE: Core = Core {
    crust: 0x232634,
    mantle: 0x292c3c,
    base: 0x303446,
    surface0: 0x414559,
    surface1: 0x51576d,
    surface2: 0x626880,
    overlay0: 0x737994,
    overlay1: 0x838ba7,
    subtext0: 0xa5adce,
    text: 0xc6d0f5,
};

const LATTE: Core = Core {
    crust: 0xdce0e8,
    mantle: 0xe6e9ef,
    base: 0xeff1f5,
    surface0: 0xccd0da,
    surface1: 0xbcc0cc,
    surface2: 0xacb0be,
    overlay0: 0x9ca0b0,
    overlay1: 0x8c8fa1,
    subtext0: 0x6c6f85,
    text: 0x4c4f69,
};

/// (name, dark-flavor hex, latte hex)
const ACCENTS: &[(&str, u32, u32)] = &[
    ("rosewater", 0xf5e0dc, 0xdc8a78),
    ("flamingo", 0xf2cdcd, 0xdd7878),
    ("pink", 0xf5c2e7, 0xea76cb),
    ("mauve", 0xcba6f7, 0x8839ef),
    ("red", 0xf38ba8, 0xd20f39),
    ("maroon", 0xeba0ac, 0xe64553),
    ("peach", 0xfab387, 0xfe640b),
    ("yellow", 0xf9e2af, 0xdf8e1d),
    ("green", 0xa6e3a1, 0x40a02b),
    ("teal", 0x94e2d5, 0x179299),
    ("sky", 0x89dceb, 0x04a5e5),
    ("sapphire", 0x74c7ec, 0x209fb5),
    ("blue", 0x89b4fa, 0x1e66f5),
    ("lavender", 0xb4befe, 0x7287fd),
];

fn accent_rgb(flavor: Flavor, name: Option<&str>) -> u32 {
    let latte = flavor == Flavor::Latte;
    let wanted = name.unwrap_or("blue");
    for (n, dark, light) in ACCENTS {
        if n.eq_ignore_ascii_case(wanted) {
            return if latte { *light } else { *dark };
        }
    }
    // Unknown accent name: fall back to blue.
    if latte {
        0x1e66f5
    } else {
        0x89b4fa
    }
}

pub fn resolve(flavor: Flavor, accent: Option<&str>) -> Palette {
    let core = match flavor {
        Flavor::Mocha => MOCHA,
        Flavor::Macchiato => MACCHIATO,
        Flavor::Frappe => FRAPPE,
        Flavor::Latte => LATTE,
    };
    Palette {
        crust: c(core.crust),
        mantle: c(core.mantle),
        base: c(core.base),
        surface0: c(core.surface0),
        surface1: c(core.surface1),
        surface2: c(core.surface2),
        overlay0: c(core.overlay0),
        overlay1: c(core.overlay1),
        subtext0: c(core.subtext0),
        text: c(core.text),
        accent: c(accent_rgb(flavor, accent)),
    }
}
