//! The physical order of the sticks, left to right, as the user identified it.
//! Detection finds sticks in address order (0x58, 0x59, ...), which need not
//! match the slots they sit in.

/// The most sticks one SMBus can hold (addresses 0x58 to 0x5F).
pub const MAX_STICKS: usize = 8;
pub const MAX_ORDER_BYTES: usize = 64;

/// Entry `p` is the index, in address order, of the stick in physical slot `p`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickOrder(Vec<usize>);

/// True when `items` holds each of `0..items.len()` exactly once, for at most 8 sticks.
fn is_permutation(items: &[usize]) -> bool {
    let mut seen = [false; MAX_STICKS];
    items.len() <= MAX_STICKS
        && items.iter().all(|&i| i < items.len() && !std::mem::replace(&mut seen[i], true))
}

impl StickOrder {
    /// Slot `p` holds stick `p`: the order detection finds them in.
    pub fn identity(n: usize) -> StickOrder {
        StickOrder((0..n).collect())
    }

    /// `picks[k]` is the slot the user chose for stick `k`. `None` unless
    /// every slot was picked exactly once.
    pub fn from_picks(picks: &[usize]) -> Option<StickOrder> {
        if picks.is_empty() || !is_permutation(picks) {
            return None;
        }
        let mut slots = vec![0; picks.len()];
        for (stick, &slot) in picks.iter().enumerate() {
            slots[slot] = stick;
        }
        Some(StickOrder(slots))
    }

    /// The saved order if it fits `n` sticks, else the identity order.
    pub fn or_identity(saved: Option<StickOrder>, n: usize) -> StickOrder {
        saved.filter(|o| o.fits(n)).unwrap_or_else(|| StickOrder::identity(n))
    }

    /// The stick (address-order index) in physical slot `p`.
    pub fn stick_at(&self, p: usize) -> Option<usize> {
        self.0.get(p).copied()
    }

    /// True when this is an order of exactly `n` sticks.
    pub fn fits(&self, n: usize) -> bool {
        self.0.len() == n && is_permutation(&self.0)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `(row, stick)` pairs: frame row `p` is drawn on the stick in slot `p`.
    /// Only the first `rows` slots get a row; sticks past them are left alone.
    pub fn frame_targets(&self, rows: usize) -> Vec<(usize, usize)> {
        self.0.iter().copied().take(rows).enumerate().collect()
    }
}

/// Returns `None` for anything other than exactly the expected file.
pub fn parse(text: &str) -> Option<StickOrder> {
    if text.len() > MAX_ORDER_BYTES {
        return None;
    }
    let mut lines = text.lines();
    if lines.next()? != "version=1" {
        return None;
    }
    let entries = lines.next()?.strip_prefix("order=")?;
    if lines.next().is_some() {
        return None;
    }
    // Each entry is one digit, so signs, spaces and leading zeros are rejected.
    let mut slots = Vec::new();
    for entry in entries.split(',') {
        match entry.as_bytes() {
            [digit @ b'0'..=b'9'] => slots.push(usize::from(digit - b'0')),
            _ => return None,
        }
    }
    let order = StickOrder(slots);
    order.fits(order.len()).then_some(order)
}

pub fn serialize(order: &StickOrder) -> String {
    let entries: Vec<String> = order.0.iter().map(usize::to_string).collect();
    format!("version=1\norder={}\n", entries.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(items: &[usize]) -> StickOrder {
        StickOrder(items.to_vec())
    }

    #[test]
    fn identity_keeps_address_order() {
        let o = StickOrder::identity(4);
        assert_eq!(o, order(&[0, 1, 2, 3]));
        assert_eq!((0..4).map(|p| o.stick_at(p)).collect::<Vec<_>>(), [Some(0), Some(1), Some(2), Some(3)]);
        assert_eq!(o.stick_at(4), None);
    }

    /// The user's board: address order is physically slot 1, 3, 2, 4.
    #[test]
    fn picks_become_slot_to_stick() {
        let o = StickOrder::from_picks(&[0, 2, 1, 3]).unwrap();
        assert_eq!(o, order(&[0, 2, 1, 3]));
        // Stick 0 in slot 1, stick 1 in slot 2, stick 2 in slot 0.
        let o = StickOrder::from_picks(&[1, 2, 0]).unwrap();
        assert_eq!(o, order(&[2, 0, 1]));
        assert_eq!(o.stick_at(0), Some(2));
    }

    #[test]
    fn picks_must_be_a_permutation() {
        for picks in [&[][..], &[0, 0], &[0, 2], &[1, 2, 3], &[0, 1, 2, 3, 4, 5, 6, 7, 8]] {
            assert_eq!(StickOrder::from_picks(picks), None, "{picks:?}");
        }
        assert!(StickOrder::from_picks(&[7, 6, 5, 4, 3, 2, 1, 0]).is_some(), "8 sticks");
    }

    #[test]
    fn fits_only_its_own_stick_count() {
        let o = order(&[0, 2, 1, 3]);
        assert!(o.fits(4));
        assert!(!o.fits(3) && !o.fits(5));
        assert!(!order(&[0, 0, 1, 2]).fits(4), "duplicate");
        assert!(!order(&[0, 1, 2, 4]).fits(4), "out of range");
    }

    #[test]
    fn saved_order_is_used_only_when_it_fits() {
        let saved = order(&[0, 2, 1, 3]);
        assert_eq!(StickOrder::or_identity(Some(saved.clone()), 4), saved);
        assert_eq!(StickOrder::or_identity(Some(saved), 3), StickOrder::identity(3));
        assert_eq!(StickOrder::or_identity(None, 2), StickOrder::identity(2));
    }

    #[test]
    fn frame_rows_go_to_the_stick_in_each_slot() {
        let o = order(&[0, 2, 1, 3]);
        assert_eq!(o.frame_targets(4), vec![(0, 0), (1, 2), (2, 1), (3, 3)]);
        // A frame holds 4 rows, so slots past it get none.
        let six = order(&[5, 4, 3, 2, 1, 0]);
        assert_eq!(six.frame_targets(4), vec![(0, 5), (1, 4), (2, 3), (3, 2)]);
        // Fewer sticks than rows: only the sticks there are.
        assert_eq!(order(&[1, 0]).frame_targets(4), vec![(0, 1), (1, 0)]);
    }

    #[test]
    fn round_trips() {
        let o = order(&[0, 2, 1, 3]);
        let text = serialize(&o);
        assert_eq!(text, "version=1\norder=0,2,1,3\n");
        assert_eq!(parse(&text), Some(o));
        assert_eq!(parse("version=1\norder=1,0\n"), Some(order(&[1, 0])));
        assert_eq!(parse("version=1\norder=7,6,5,4,3,2,1,0\n").map(|o| o.len()), Some(8));
    }

    #[test]
    fn rejects_bad_input() {
        let oversized = format!("version=1\norder=0,1\n{}", " ".repeat(MAX_ORDER_BYTES));
        let cases = [
            "",
            "version=1\n",
            "version=2\norder=0,1\n",
            "version=1\norder=\n",
            "version=1\norder=0,0\n",
            "version=1\norder=0,2\n",
            "version=1\norder=1,2,3\n",
            "version=1\norder=0,1,2,3,4,5,6,7,8\n",
            "version=1\norder=0,1,\n",
            "version=1\norder=0,,1\n",
            "version=1\norder=0, 1\n",
            "version=1\norder=+0,1\n",
            "version=1\norder=00,1\n",
            "version=1\norder=-1,0\n",
            "version=1\norder=a,b\n",
            "version=1\norders=0,1\n",
            "version=1\norder=0,1\nextra=1\n",
            "order=0,1\nversion=1\n",
            &oversized,
        ];
        for case in cases {
            assert_eq!(parse(case), None, "accepted {case:?}");
        }
    }
}
