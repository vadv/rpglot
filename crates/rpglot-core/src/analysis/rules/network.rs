use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::DataBlock;

use super::AnalysisRule;

// ============================================================
// NetworkSpikeRule
// ============================================================

pub struct NetworkSpikeRule;

impl AnalysisRule for NetworkSpikeRule {
    fn id(&self) -> &'static str {
        "network_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev = match ctx.prev {
            Some(p) => p,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(nets) = find_block(ctx.snapshot, |b| match b {
            DataBlock::SystemNet(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let total_rx: u64 = nets.iter().map(|n| n.rx_bytes).sum();
        let total_tx: u64 = nets.iter().map(|n| n.tx_bytes).sum();
        let rx_d = total_rx.saturating_sub(prev.net_rx_bytes);
        let tx_d = total_tx.saturating_sub(prev.net_tx_bytes);
        let bytes_s = (rx_d + tx_d) as f64 / ctx.dt;

        let avg = ctx.ewma.net_rx_bytes_s + ctx.ewma.net_tx_bytes_s;
        if !ctx.ewma.is_spike(bytes_s, avg, 2.0) {
            return Vec::new();
        }

        let mb_s = bytes_s / 1_048_576.0;
        let avg_mb_s = avg / 1_048_576.0;
        let factor = if avg > 0.0 { bytes_s / avg } else { 0.0 };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "network_spike",
            category: Category::Network,
            severity: Severity::Warning,
            title: format!("Network traffic spike {mb_s:.1} MB/s ({factor:.1}x above normal)",),
            detail: Some(format!(
                "Current: {mb_s:.1} MB/s, baseline avg: {avg_mb_s:.1} MB/s",
            )),
            value: bytes_s,
        }]
    }
}
