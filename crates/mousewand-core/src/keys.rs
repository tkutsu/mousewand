//! Key combos like `"ctrl+shift+t"`, turned into Linux evdev key codes.
//!
//! Codes are physical keys, so letters follow the US QWERTY layout no matter
//! which layout the compositor applies.

use std::fmt;
use std::str::FromStr;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Combo {
    text: String,
    /// Key codes in press order: modifiers first, the main key last.
    pub codes: Vec<u16>,
}

impl FromStr for Combo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let codes = s
            .split('+')
            .map(|name| code(name.trim()).ok_or_else(|| format!("unknown key {name:?} in {s:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Combo {
            text: s.to_string(),
            codes,
        })
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// The evdev code for a key name, case-insensitive.
pub fn code(name: &str) -> Option<u16> {
    let name = name.to_ascii_lowercase();
    if let Some(c) = single_char(&name) {
        return Some(c);
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        return match n {
            1..=10 => Some(58 + n),
            11 => Some(87),
            12 => Some(88),
            13..=24 => Some(170 + n),
            _ => None,
        };
    }
    Some(match name.as_str() {
        "ctrl" | "control" | "lctrl" => 29,
        "rctrl" => 97,
        "shift" | "lshift" => 42,
        "rshift" => 54,
        "alt" | "lalt" => 56,
        "ralt" | "altgr" => 100,
        "super" | "meta" | "win" | "logo" => 125,
        "esc" | "escape" => 1,
        "backspace" => 14,
        "tab" => 15,
        "enter" | "return" => 28,
        "space" => 57,
        "capslock" => 58,
        "minus" => 12,
        "equal" => 13,
        "leftbrace" | "bracketleft" => 26,
        "rightbrace" | "bracketright" => 27,
        "semicolon" => 39,
        "apostrophe" => 40,
        "grave" => 41,
        "backslash" => 43,
        "comma" => 51,
        "dot" | "period" => 52,
        "slash" => 53,
        "print" | "sysrq" => 99,
        "pause" => 119,
        "home" => 102,
        "up" => 103,
        "pageup" | "pgup" => 104,
        "left" => 105,
        "right" => 106,
        "end" => 107,
        "down" => 108,
        "pagedown" | "pgdn" => 109,
        "insert" => 110,
        "delete" | "del" => 111,
        "menu" | "compose" => 127,
        "mute" => 113,
        "volumedown" => 114,
        "volumeup" => 115,
        "nextsong" | "next" => 163,
        "playpause" | "play" => 164,
        "previoussong" | "prev" => 165,
        "back" => 158,
        "forward" => 159,
        "brightnessdown" => 224,
        "brightnessup" => 225,
        _ => return None,
    })
}

/// Modifier keys held on the keyboard while drawing, written like
/// `"ctrl+shift"`. Left and right count the same.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct Mods(u8);

/// Each modifier's name, its bit, and its left and right key codes.
const MODS: [(&str, u8, [u16; 2]); 4] = [
    ("ctrl", 1, [29, 97]),
    ("shift", 2, [42, 54]),
    ("alt", 4, [56, 100]),
    ("super", 8, [125, 126]),
];

impl Mods {
    pub const NONE: Mods = Mods(0);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The modifiers among these held key codes.
    pub fn of_keys(held: impl IntoIterator<Item = u16>) -> Mods {
        let mut mods = 0;
        for code in held {
            if let Some((_, bit, _)) = MODS.iter().find(|(_, _, codes)| codes.contains(&code)) {
                mods |= bit;
            }
        }
        Mods(mods)
    }

    /// Every key code that counts as one of the modifiers.
    pub fn codes() -> impl Iterator<Item = u16> {
        MODS.iter().flat_map(|(_, _, codes)| *codes)
    }
}

impl FromStr for Mods {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let mut mods = 0;
        for name in s.split('+').map(|n| n.trim().to_ascii_lowercase()) {
            let name = match name.as_str() {
                "control" => "ctrl",
                "meta" | "win" | "logo" => "super",
                other => other,
            };
            match MODS.iter().find(|(n, _, _)| *n == name) {
                Some((_, bit, _)) => mods |= bit,
                None => return Err(format!("{name:?} isn't a modifier: use ctrl, shift, alt or super")),
            }
        }
        Ok(Mods(mods))
    }
}

impl fmt::Display for Mods {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let names: Vec<&str> = MODS
            .iter()
            .filter(|(_, bit, _)| self.0 & bit != 0)
            .map(|(name, _, _)| *name)
            .collect();
        f.write_str(&names.join("+"))
    }
}

fn single_char(name: &str) -> Option<u16> {
    let mut chars = name.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    const ROWS: [(&str, u16); 4] = [
        ("1234567890", 2),
        ("qwertyuiop", 16),
        ("asdfghjkl", 30),
        ("zxcvbnm", 44),
    ];
    if let Some(code) = ROWS
        .iter()
        .find_map(|(row, first)| row.find(c).map(|i| first + i as u16))
    {
        return Some(code);
    }
    Some(match c {
        '-' => 12,
        '=' => 13,
        '[' => 26,
        ']' => 27,
        ';' => 39,
        '\'' => 40,
        '`' => 41,
        '\\' => 43,
        ',' => 51,
        '.' => 52,
        '/' => 53,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos() {
        assert_eq!("alt+Left".parse::<Combo>().unwrap().codes, [56, 105]);
        assert_eq!("ctrl+shift+t".parse::<Combo>().unwrap().codes, [29, 42, 20]);
        assert_eq!("super+1".parse::<Combo>().unwrap().codes, [125, 2]);
        assert_eq!("F5".parse::<Combo>().unwrap().codes, [63]);
        assert_eq!("F12".parse::<Combo>().unwrap().codes, [88]);
        assert_eq!("ctrl+/".parse::<Combo>().unwrap().codes, [29, 53]);
        assert!("ctrl+nope".parse::<Combo>().is_err());
    }

    #[test]
    fn modifiers() {
        let mods: Mods = "Shift+ctrl".parse().unwrap();
        assert_eq!(mods.to_string(), "ctrl+shift");
        assert_eq!(Mods::of_keys([54, 97, 30]), mods, "right keys count, other keys don't");
        assert!(Mods::of_keys([30]).is_empty());
        assert!("ctrl+t".parse::<Mods>().unwrap_err().contains("isn't a modifier"));
    }

    #[test]
    fn letters_follow_qwerty_codes() {
        assert_eq!(code("q"), Some(16));
        assert_eq!(code("a"), Some(30));
        assert_eq!(code("m"), Some(50));
        assert_eq!(code("0"), Some(11));
    }
}
