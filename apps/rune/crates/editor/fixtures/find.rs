// Totals the order lines.
pub struct Order {
    pub lines: Vec<Line>,
    pub total_cents: u64,
}

pub struct Line {
    pub sku: String,
    pub cents: u64,
    pub quantity: u32,
}

impl Order {
    /// The order total: every line's cents times its quantity.
    pub fn total(&self) -> u64 {
        self.lines.iter().map(|line| line.cents * u64::from(line.quantity)).sum()
    }

    pub fn totals_match(&self) -> bool {
        self.total() == self.total_cents
    }
}
