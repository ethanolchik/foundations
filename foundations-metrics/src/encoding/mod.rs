mod text;

use foundations_metrics_registry::proto::MetricType;
use prost::Message;

use crate::MetricFamily;

pub use text::encode_to_text;

/// Encodes metric families as length-delimited Prometheus protobuf messages.
pub fn encode_to_protobuf(families: &[MetricFamily]) -> Vec<u8> {
    families
        .iter()
        .filter(|family| family.r#type != Some(MetricType::Summary as i32))
        .flat_map(Message::encode_length_delimited_to_vec)
        .collect()
}

#[cfg(test)]
mod tests {
    use foundations_metrics_registry::proto::{Gauge, LabelPair, Metric};

    use super::*;

    #[test]
    fn omits_summary_families() {
        let families = [MetricFamily {
            name: Some("request_size".to_owned()),
            help: Some("Request size.".to_owned()),
            r#type: Some(MetricType::Summary as i32),
            metric: vec![Metric {
                summary: Some(Default::default()),
                ..Default::default()
            }],
            unit: None,
        }];

        assert!(encode_to_protobuf(&families).is_empty());
    }

    #[test]
    fn preserves_legacy_info_gauge_representation() {
        let families = [MetricFamily {
            name: Some("build_info".to_owned()),
            help: Some("Build information.".to_owned()),
            r#type: Some(MetricType::Gauge as i32),
            metric: vec![Metric {
                label: vec![LabelPair {
                    name: Some("version".to_owned()),
                    value: Some("1.2.3".to_owned()),
                }],
                gauge: Some(Gauge { value: Some(1.0) }),
                ..Default::default()
            }],
            unit: None,
        }];

        let encoded = encode_to_protobuf(&families);
        let decoded = MetricFamily::decode_length_delimited(encoded.as_slice()).unwrap();

        assert_eq!(decoded, families[0]);
    }
}
