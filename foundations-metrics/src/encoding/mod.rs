mod text;

use foundations_metrics_registry::proto::MetricType;
use prost::Message;

use crate::MetricFamily;
use crate::validation::{ValidationContext, sanitized_metric_family};

pub use text::encode_to_text;

/// Encodes metric families as length-delimited Prometheus protobuf messages.
pub fn encode_to_protobuf(families: &[MetricFamily]) -> Vec<u8> {
    let mut output = Vec::new();
    for family in families {
        if family.r#type == Some(MetricType::Summary as i32) {
            continue;
        }
        if let Some(family) = sanitized_metric_family(family, ValidationContext::ProtobufEncoding) {
            family
                .encode_length_delimited(&mut output)
                .expect("encoding a protobuf message to a Vec cannot fail");
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use foundations_metrics_registry::proto::{
        Bucket, Counter, Exemplar, Gauge, Histogram, LabelPair, Metric, MetricType,
    };

    use super::*;

    fn label(name: &str, value: &str) -> LabelPair {
        LabelPair {
            name: Some(name.to_owned()),
            value: Some(value.to_owned()),
        }
    }

    fn decode_families(mut bytes: &[u8]) -> Vec<MetricFamily> {
        let mut families = Vec::new();
        while !bytes.is_empty() {
            families.push(
                MetricFamily::decode_length_delimited(&mut bytes)
                    .expect("encoded family should decode"),
            );
        }
        families
    }

    #[test]
    fn fully_valid_protobuf_output_is_unchanged() {
        let families = [MetricFamily {
            name: Some("valid:counter".to_owned()),
            help: Some("Valid counter.".to_owned()),
            r#type: Some(MetricType::Counter as i32),
            metric: vec![Metric {
                label: vec![label("_label", "value")],
                counter: Some(Counter {
                    value: Some(1.0),
                    exemplar: Some(Exemplar::default()),
                    created_timestamp: None,
                }),
                ..Default::default()
            }],
            unit: None,
        }];
        let expected: Vec<_> = families
            .iter()
            .flat_map(Message::encode_length_delimited_to_vec)
            .collect();

        let encoded = encode_to_protobuf(&families);
        assert_eq!(encoded, expected);
        assert!(
            decode_families(&encoded)[0].metric[0]
                .counter
                .as_ref()
                .unwrap()
                .exemplar
                .is_some(),
            "empty exemplars retain their existing protobuf behavior"
        );
    }

    #[test]
    fn protobuf_defensively_omits_invalid_families_and_rows_and_strips_exemplars() {
        let families = [
            MetricFamily {
                name: Some("bad\nfamily".to_owned()),
                help: None,
                r#type: Some(MetricType::Gauge as i32),
                metric: vec![Metric {
                    gauge: Some(Gauge { value: Some(99.0) }),
                    ..Default::default()
                }],
                unit: None,
            },
            MetricFamily {
                name: Some("protobuf_counter".to_owned()),
                help: None,
                r#type: Some(MetricType::Counter as i32),
                metric: vec![
                    Metric {
                        label: vec![label("id", "kept")],
                        counter: Some(Counter {
                            value: Some(1.0),
                            exemplar: Some(Exemplar {
                                label: vec![label("trace:id", "bad")],
                                ..Default::default()
                            }),
                            created_timestamp: None,
                        }),
                        ..Default::default()
                    },
                    Metric {
                        label: vec![label("bad name", "dropped")],
                        counter: Some(Counter {
                            value: Some(2.0),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    Metric {
                        label: vec![label("dup", "a"), label("dup", "b")],
                        counter: Some(Counter {
                            value: Some(3.0),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                ],
                unit: None,
            },
            MetricFamily {
                name: Some("protobuf_histogram".to_owned()),
                help: None,
                r#type: Some(MetricType::Histogram as i32),
                metric: vec![
                    Metric {
                        histogram: Some(Histogram {
                            bucket: vec![Bucket {
                                exemplar: Some(Exemplar {
                                    label: vec![label("dup", "a"), label("dup", "b")],
                                    ..Default::default()
                                }),
                                ..Default::default()
                            }],
                            exemplars: vec![
                                Exemplar {
                                    label: vec![label("bad#name", "bad")],
                                    ..Default::default()
                                },
                                Exemplar {
                                    label: vec![label("trace_id", "good")],
                                    ..Default::default()
                                },
                            ],
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    Metric {
                        label: vec![label("le", "1")],
                        histogram: Some(Histogram::default()),
                        ..Default::default()
                    },
                ],
                unit: None,
            },
            MetricFamily {
                name: Some("protobuf_sibling".to_owned()),
                help: None,
                r#type: Some(MetricType::Gauge as i32),
                metric: vec![Metric {
                    gauge: Some(Gauge { value: Some(4.0) }),
                    ..Default::default()
                }],
                unit: None,
            },
        ];

        let decoded = decode_families(&encode_to_protobuf(&families));
        assert_eq!(
            decoded
                .iter()
                .filter_map(|family| family.name.as_deref())
                .collect::<Vec<_>>(),
            [
                "protobuf_counter",
                "protobuf_histogram",
                "protobuf_sibling",
            ]
        );

        assert_eq!(decoded[0].metric.len(), 1);
        assert!(
            decoded[0].metric[0]
                .counter
                .as_ref()
                .unwrap()
                .exemplar
                .is_none()
        );

        assert_eq!(decoded[1].metric.len(), 1);
        let histogram = decoded[1].metric[0].histogram.as_ref().unwrap();
        assert!(histogram.bucket[0].exemplar.is_none());
        assert_eq!(histogram.exemplars.len(), 1);
        assert_eq!(
            histogram.exemplars[0].label[0].name.as_deref(),
            Some("trace_id")
        );

        assert_eq!(decoded[2].metric.len(), 1);
    }

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
