use crate::effects::{Direction, Effect, Rgb, Settings, Speed, BRIGHTNESS_RANGE, MAX_COLORS, SPEED_PCT_RANGE};

pub const MAX_SETTINGS_BYTES: usize = 1024;

fn parse_color(text: &str) -> Option<Rgb> {
    if text.len() != 6 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(text, 16).ok()?;
    Some(Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

fn next_value<'a>(lines: &mut std::str::Lines<'a>, key: &str) -> Option<&'a str> {
    lines.next()?.strip_prefix(key)?.strip_prefix('=')
}

/// A whole number from `min` to `max`, written as `serialize` writes it:
/// digits only, at most 3, and no leading zeros.
fn parse_number(text: &str, min: u16, max: u16) -> Option<u16> {
    if text.is_empty() || text.len() > 3 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    let value: u16 = text.parse().ok()?;
    (min..=max).contains(&value).then_some(value)
}

/// Returns `None` for anything other than exactly the expected file.
pub fn parse(text: &str) -> Option<Settings> {
    if text.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    let mut lines = text.lines();
    // Version 1 predates the sync toggle and has no `synced` line.
    // Version 2 predates the extra animation settings, from `colors3` on.
    let version = match next_value(&mut lines, "version")? {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        _ => return None,
    };
    let effect = Effect::from_name(next_value(&mut lines, "effect")?)?;
    let color1 = parse_color(next_value(&mut lines, "color1")?)?;
    let color2 = parse_color(next_value(&mut lines, "color2")?)?;
    let random = parse_flag(next_value(&mut lines, "random")?)?;
    let speed = Speed::from_name(next_value(&mut lines, "speed")?)?;
    let direction = Direction::from_name(next_value(&mut lines, "direction")?)?;
    // A version 1 file was saved before sync existed, so the RAM ran it: keep that.
    let synced = if version >= 2 { parse_flag(next_value(&mut lines, "synced")?)? } else { false };
    let defaults = Settings::default();
    let mut settings = Settings {
        effect, color1, color2, random, speed, direction, synced,
        color_count: effect.color_count(),
        ..defaults
    };
    if version == 3 {
        settings.more_colors = [
            parse_color(next_value(&mut lines, "colors3")?)?,
            parse_color(next_value(&mut lines, "colors4")?)?,
        ];
        settings.color_count = parse_number(next_value(&mut lines, "color_count")?, 0, MAX_COLORS.into())? as u8;
        settings.speed_pct = parse_number(next_value(&mut lines, "speed_pct")?, SPEED_PCT_RANGE.0, SPEED_PCT_RANGE.1)?;
        let (low, high) = BRIGHTNESS_RANGE;
        settings.brightness = parse_number(next_value(&mut lines, "brightness")?, low.into(), high.into())? as u8;
        settings.reverse = parse_flag(next_value(&mut lines, "reverse")?)?;
    }
    if lines.next().is_some() {
        return None;
    }
    Some(settings.normalized())
}

fn parse_flag(text: &str) -> Option<bool> {
    match text {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

pub fn serialize(settings: &Settings) -> String {
    let hex = |c: Rgb| format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2);
    format!(
        "version=3\neffect={}\ncolor1={}\ncolor2={}\nrandom={}\nspeed={}\ndirection={}\nsynced={}\n\
         colors3={}\ncolors4={}\ncolor_count={}\nspeed_pct={}\nbrightness={}\nreverse={}\n",
        settings.effect.name(),
        hex(settings.color1),
        hex(settings.color2),
        settings.random as u8,
        settings.speed.name(),
        settings.direction.name(),
        settings.synced as u8,
        hex(settings.more_colors[0]),
        hex(settings.more_colors[1]),
        settings.color_count,
        settings.speed_pct,
        settings.brightness,
        settings.reverse as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> String {
        serialize(&Settings::default())
    }

    /// The version 3 lines after `synced=`, as `Settings::default()` writes them.
    const V3_LINES: &str = "colors3=7CFFB2\ncolors4=C77DFF\ncolor_count=0\nspeed_pct=100\nbrightness=100\nreverse=0\n";

    /// A version 2 file: no lines after `synced=`.
    fn v2(text: &str) -> String {
        let end = text.find("colors3=").unwrap();
        text[..end].replace("version=3", "version=2")
    }

    #[test]
    fn round_trips() {
        assert_eq!(parse(&valid()), Some(Settings::default()));
        let synced = Settings { effect: Effect::ColorWave, synced: true, ..Settings::default() }.normalized();
        let text = serialize(&synced);
        assert!(text.starts_with("version=3\n") && text.contains("direction=down\nsynced=1\ncolors3="), "{text:?}");
        assert_eq!(parse(&text), Some(synced));
    }

    #[test]
    fn writes_the_extra_settings_after_synced() {
        let s = Settings {
            effect: Effect::Gradient,
            more_colors: [Rgb(1, 2, 3), Rgb(0xAB, 0xCD, 0xEF)],
            color_count: 4,
            speed_pct: 125,
            brightness: 80,
            reverse: true,
            ..Settings::default()
        }.normalized();
        let text = serialize(&s);
        assert!(text.ends_with("synced=0\ncolors3=010203\ncolors4=ABCDEF\ncolor_count=4\nspeed_pct=125\nbrightness=80\nreverse=1\n"), "{text:?}");
        assert_eq!(parse(&text), Some(s));
        assert!(valid().ends_with(&format!("synced=1\n{V3_LINES}")));
    }

    /// Files saved before the sync toggle keep working, as not synced.
    #[test]
    fn accepts_version_1_without_synced() {
        let v1 = v2(&valid()).replace("version=2", "version=1").replace("synced=1\n", "");
        assert_eq!(parse(&v1), Some(Settings { synced: false, ..Settings::default() }));
        assert_eq!(parse(&v2(&valid()).replace("version=2", "version=1")), None, "v1 has no synced line");
    }

    /// Files saved before the extra settings keep the user's choices and get defaults for the rest.
    #[test]
    fn accepts_version_2_with_defaults() {
        for (effect, synced) in [(Effect::ColorWave, false), (Effect::ColorWave, true), (Effect::Breathing, false), (Effect::Static, false)] {
            let s = Settings { effect, synced, ..Settings::default() }.normalized();
            let parsed = parse(&v2(&serialize(&s))).unwrap();
            assert_eq!(parsed.synced, synced, "{effect:?}");
            assert_eq!(parsed.color_count, effect.color_count(), "{effect:?}");
            assert_eq!((parsed.speed_pct, parsed.brightness, parsed.reverse), (100, 100, false));
            assert_eq!(parsed.more_colors, Settings::default().more_colors);
        }
        assert_eq!(parse(&valid().replace("version=3", "version=2")), None, "v2 has no extra lines");
        assert_eq!(parse(&v2(&valid()).replace("version=2", "version=3")), None, "v3 needs them");
    }

    #[test]
    fn rejects_out_of_range_values() {
        let cases = [
            ("speed_pct=100", "speed_pct=20"),
            ("speed_pct=100", "speed_pct=305"),
            ("speed_pct=100", "speed_pct=1000"),
            ("brightness=100", "brightness=5"),
            ("brightness=100", "brightness=101"),
            ("color_count=0", "color_count=5"),
            ("speed_pct=100", "speed_pct=0100"),
            ("brightness=100", "brightness=+90"),
            ("brightness=100", "brightness=-1"),
            ("brightness=100", "brightness="),
            ("color_count=0", "color_count=00"),
            ("reverse=0", "reverse=2"),
            ("colors3=7CFFB2", "colors3=7CFFBZ"),
            ("colors4=C77DFF", "colors4=C77DF"),
        ];
        for (from, to) in cases {
            let text = valid().replace(from, to);
            assert_ne!(text, valid(), "{from}");
            assert_eq!(parse(&text), None, "accepted {to}");
        }
    }

    #[test]
    fn clamps_the_colour_count_to_the_effect() {
        let text = valid().replace("rainbow-wave", "static").replace("color_count=0", "color_count=4");
        assert_eq!(parse(&text).unwrap().color_count, 1);
        let text = valid().replace("rainbow-wave", "chase");
        assert_eq!(parse(&text).unwrap().color_count, 2, "0 means Chase's own count");
    }

    #[test]
    fn rejects_bad_input() {
        let oversized = format!("{}{}", valid(), " ".repeat(MAX_SETTINGS_BYTES));
        let cases = [
            String::new(),
            valid().replace("version=3", "version=4"),
            valid().replace("synced=1\n", ""),
            valid().replace("synced=1", "synced=2"),
            valid().replace("synced=1", "synced=true"),
            valid().replace("synced=1", "sync=1"),
            valid().replace("reverse=0\n", ""),
            valid().replace("colors3=7CFFB2\ncolors4=C77DFF", "colors4=C77DFF\ncolors3=7CFFB2"),
            valid().replace("rainbow-wave", "../../evil"),
            valid().replace("2F9BFF", "2F9BFG"),
            valid().replace("2F9BFF", "2F9BF"),
            valid().replace("random=0", "random=2"),
            valid().replace("speed=slow", "speed=warp"),
            format!("{}extra=1\n", valid()),
            format!("{}extra=1\n", v2(&valid())),
            oversized,
        ];
        for case in cases {
            assert_eq!(parse(&case), None, "accepted: {case:?}");
        }
    }

    #[test]
    fn removes_unsupported_options() {
        let text = valid().replace("rainbow-wave", "rainbow").replace("random=0", "random=1");
        assert!(!parse(&text).unwrap().random);
    }
}
