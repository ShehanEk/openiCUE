use crate::effects::{Direction, Effect, Rgb, Settings, Speed};

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

/// Returns `None` for anything other than exactly the expected file.
pub fn parse(text: &str) -> Option<Settings> {
    if text.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    let mut lines = text.lines();
    // Version 1 predates the sync toggle and has no `synced` line.
    let version = match next_value(&mut lines, "version")? {
        "1" => 1,
        "2" => 2,
        _ => return None,
    };
    let effect = Effect::from_name(next_value(&mut lines, "effect")?)?;
    let color1 = parse_color(next_value(&mut lines, "color1")?)?;
    let color2 = parse_color(next_value(&mut lines, "color2")?)?;
    let random = parse_flag(next_value(&mut lines, "random")?)?;
    let speed = Speed::from_name(next_value(&mut lines, "speed")?)?;
    let direction = Direction::from_name(next_value(&mut lines, "direction")?)?;
    let synced = if version == 2 { parse_flag(next_value(&mut lines, "synced")?)? } else { false };
    if lines.next().is_some() {
        return None;
    }
    Some(Settings { effect, color1, color2, random, speed, direction, synced }.normalized())
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
        "version=2\neffect={}\ncolor1={}\ncolor2={}\nrandom={}\nspeed={}\ndirection={}\nsynced={}\n",
        settings.effect.name(),
        hex(settings.color1),
        hex(settings.color2),
        settings.random as u8,
        settings.speed.name(),
        settings.direction.name(),
        settings.synced as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> String {
        serialize(&Settings::default())
    }

    #[test]
    fn round_trips() {
        assert_eq!(parse(&valid()), Some(Settings::default()));
        let synced = Settings { effect: Effect::ColorWave, synced: true, ..Settings::default() };
        let text = serialize(&synced);
        assert!(text.starts_with("version=2\n") && text.ends_with("direction=down\nsynced=1\n"), "{text:?}");
        assert_eq!(parse(&text), Some(synced));
    }

    /// Files saved before the sync toggle keep working, as not synced.
    #[test]
    fn accepts_version_1_without_synced() {
        let v1 = valid().replace("version=2", "version=1").replace("synced=0\n", "");
        assert_eq!(parse(&v1), Some(Settings::default()));
        assert_eq!(parse(&valid().replace("version=2", "version=1")), None, "v1 has no synced line");
    }

    #[test]
    fn rejects_bad_input() {
        let oversized = format!("{}{}", valid(), " ".repeat(MAX_SETTINGS_BYTES));
        let cases = [
            String::new(),
            valid().replace("version=2", "version=3"),
            valid().replace("synced=0\n", ""),
            valid().replace("synced=0", "synced=2"),
            valid().replace("synced=0", "synced=true"),
            valid().replace("synced=0", "sync=0"),
            valid().replace("rainbow-wave", "../../evil"),
            valid().replace("2F9BFF", "2F9BFG"),
            valid().replace("2F9BFF", "2F9BF"),
            valid().replace("random=0", "random=2"),
            valid().replace("speed=slow", "speed=warp"),
            format!("{}extra=1\n", valid()),
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
