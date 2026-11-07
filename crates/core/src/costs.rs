//! The last measured cost of each custom effect, shown before the user applies it.
use crate::effects::{Effect, ALL_EFFECTS};

pub const MAX_COSTS_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cost {
    /// Private memory in MB.
    pub mb: f32,
    /// Share of all logical processors, in percent, like Task Manager.
    pub cpu: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Costs {
    entries: Vec<(Effect, Cost)>,
}

impl Costs {
    pub fn get(&self, effect: Effect) -> Option<Cost> {
        self.entries.iter().find(|(e, _)| *e == effect).map(|(_, cost)| *cost)
    }

    /// Ignores built-in effects, which cost nothing once applied, and impossible values.
    pub fn set(&mut self, effect: Effect, cost: Cost) {
        if !effect.is_custom() || !valid(cost) {
            return;
        }
        match self.entries.iter_mut().find(|(e, _)| *e == effect) {
            Some(entry) => entry.1 = cost,
            None => self.entries.push((effect, cost)),
        }
    }
}

fn valid(cost: Cost) -> bool {
    (0.0..=4096.0).contains(&cost.mb) && (0.0..=100.0).contains(&cost.cpu)
}

/// Digits with at most one dot, so signs, "inf" and "NaN" are rejected.
fn number(text: &str) -> Option<f32> {
    let digits_and_dot = text.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    let dots = text.bytes().filter(|&b| b == b'.').count();
    if text.is_empty() || text.len() > 8 || !digits_and_dot || dots > 1 {
        return None;
    }
    text.parse().ok()
}

/// Returns `None` for anything other than exactly the expected file.
pub fn parse(text: &str) -> Option<Costs> {
    if text.len() > MAX_COSTS_BYTES {
        return None;
    }
    let mut lines = text.lines();
    if lines.next()? != "version=1" {
        return None;
    }
    let mut costs = Costs::default();
    for line in lines {
        let (name, values) = line.split_once('=')?;
        let effect = Effect::from_name(name).filter(|e| e.is_custom())?;
        if costs.get(effect).is_some() {
            return None;
        }
        let (mb, cpu) = values.split_once(',')?;
        let cost = Cost { mb: number(mb)?, cpu: number(cpu)? };
        if !valid(cost) {
            return None;
        }
        costs.entries.push((effect, cost));
    }
    Some(costs)
}

pub fn serialize(costs: &Costs) -> String {
    let mut text = String::from("version=1\n");
    for effect in ALL_EFFECTS {
        if let Some(cost) = costs.get(effect) {
            text.push_str(&format!("{}={:.1},{:.1}\n", effect.name(), cost.mb, cost.cpu));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::Effect;

    fn sample() -> Costs {
        let mut costs = Costs::default();
        costs.set(Effect::Breathing, Cost { mb: 1.4, cpu: 0.8 });
        costs.set(Effect::Chase, Cost { mb: 1.5, cpu: 0.6 });
        costs
    }

    #[test]
    fn round_trips() {
        let text = serialize(&sample());
        assert_eq!(text, "version=1\nbreathing=1.4,0.8\nchase=1.5,0.6\n");
        assert_eq!(parse(&text), Some(sample()));
    }

    #[test]
    fn set_replaces_and_ignores_built_in_or_invalid() {
        let mut costs = sample();
        costs.set(Effect::Breathing, Cost { mb: 2.0, cpu: 1.0 });
        assert_eq!(costs.get(Effect::Breathing), Some(Cost { mb: 2.0, cpu: 1.0 }));
        costs.set(Effect::Static, Cost { mb: 1.0, cpu: 1.0 });
        assert_eq!(costs.get(Effect::Static), None);
        costs.set(Effect::Gradient, Cost { mb: f32::NAN, cpu: 1.0 });
        costs.set(Effect::Gradient, Cost { mb: 1.0, cpu: 101.0 });
        assert_eq!(costs.get(Effect::Gradient), None);
    }

    #[test]
    fn rejects_bad_input() {
        let good = serialize(&sample());
        let cases = [
            String::new(),
            good.replace("version=1", "version=2"),
            good.replace("breathing", "static"),
            good.replace("breathing", "../evil"),
            good.replace("chase", "breathing"),
            good.replace("1.4", "-1.4"),
            good.replace("1.4", "inf"),
            good.replace("1.4", "NaN"),
            good.replace("1.4", "1.4.1"),
            good.replace("0.8", "100.1"),
            good.replace("1.4,0.8", "1.4"),
            good.replace("1.4", "123456789"),
            format!("{good}{}", "x".repeat(MAX_COSTS_BYTES)),
        ];
        for case in cases {
            assert_eq!(parse(&case), None, "accepted {case:?}");
        }
    }
}
