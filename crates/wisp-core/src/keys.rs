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
        Ok(Combo { text: s.to_string(), codes })
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

fn single_char(name: &str) -> Option<u16> {
    let mut chars = name.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    const ROWS: [(&str, u16); 4] = [("1234567890", 2), ("qwertyuiop", 16), ("asdfghjkl", 30), ("zxcvbnm", 44)];
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
    fn letters_follow_qwerty_codes() {
        assert_eq!(code("q"), Some(16));
        assert_eq!(code("a"), Some(30));
        assert_eq!(code("m"), Some(50));
        assert_eq!(code("0"), Some(11));
    }
}
