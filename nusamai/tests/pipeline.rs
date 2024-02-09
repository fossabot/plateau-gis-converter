use std::sync::Once;

use nusamai::parameters::Parameters;
use nusamai::pipeline::{self, Parcel, Receiver};
use nusamai::pipeline::{Feedback, FeedbackMessage, Result, Sender};
use nusamai::sink::{DataSink, DataSinkProvider, SinkInfo};
use nusamai::source::{DataSource, DataSourceProvider, SourceInfo};
use nusamai::transformer::{self, Transformer};
use nusamai_citygml::schema::Schema;
use nusamai_plateau::Entity;
use rand::prelude::*;

static INIT: Once = Once::new();

pub struct DummySourceProvider {}

impl DataSourceProvider for DummySourceProvider {
    fn create(&self, _params: &Parameters) -> Box<dyn DataSource> {
        Box::new(DummySource {})
    }

    fn info(&self) -> SourceInfo {
        SourceInfo {
            name: "Dummy Source".to_string(),
        }
    }

    fn parameters(&self) -> Parameters {
        Parameters::default()
    }
}

pub struct DummySource {}

impl DataSource for DummySource {
    fn run(&mut self, sink: Sender, feedback: &Feedback) -> Result<()> {
        for _i in 0..100 {
            feedback.ensure_not_canceled()?;
            std::thread::sleep(std::time::Duration::from_millis(
                (5.0 + random::<f32>() * 10.0) as u64,
            ));
            let obj = Parcel {
                entity: Entity {
                    root: nusamai_citygml::Value::Double(0.),
                    geometry_store: Default::default(),
                    appearance_store: Default::default(),
                },
            };
            feedback.feedback_raw(FeedbackMessage {
                message: format!("generating: {:?}", obj),
                ..Default::default()
            });
            if sink.send(obj).is_err() {
                break;
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct NoopTransformer {}

impl Transformer for NoopTransformer {
    fn run(&self, upstream: Receiver, downstream: Sender, _feedback: &Feedback) -> Result<()> {
        for parcel in upstream {
            if downstream.send(parcel).is_err() {
                break;
            }
        }
        Ok(())
    }
}

struct DummySinkProvider {}

impl DataSinkProvider for DummySinkProvider {
    fn create(&self, _params: &Parameters) -> Box<dyn DataSink> {
        Box::new(DummySink {})
    }

    fn info(&self) -> SinkInfo {
        SinkInfo {
            name: "Dummy Sink".to_string(),
        }
    }

    fn parameters(&self) -> Parameters {
        Parameters::default()
    }
}

struct DummySink {}

impl DataSink for DummySink {
    fn run(&mut self, upstream: Receiver, feedback: &Feedback, _schema: &Schema) -> Result<()> {
        for parcel in upstream {
            feedback.ensure_not_canceled()?;

            std::thread::sleep(std::time::Duration::from_millis(
                (5.0 + random::<f32>() * 20.0) as u64,
            ));
            feedback.feedback_raw(FeedbackMessage {
                message: format!("dummy sink received: {:?}", parcel),
                ..Default::default()
            })
        }

        Ok(())
    }

    fn make_transform_requirements(&self) -> transformer::Requirements {
        transformer::Requirements {
            ..Default::default()
        }
    }
}

#[test]
fn test_run_pipeline() {
    INIT.call_once(|| {
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", "error")
        }
        pretty_env_logger::init();
    });

    let source_provider: Box<dyn DataSourceProvider> = Box::new(DummySourceProvider {});
    let sink_provider: Box<dyn DataSinkProvider> = Box::new(DummySinkProvider {});

    let source = source_provider.create(&source_provider.parameters());
    let sink = sink_provider.create(&source_provider.parameters());

    let schema = nusamai_citygml::schema::Schema::default();
    let transformer = Box::<NoopTransformer>::default();

    // start the pipeline
    let (handle, watcher, canceller) = pipeline::run(source, transformer, sink, schema.into());

    std::thread::scope(|scope| {
        // cancel the pipeline
        scope.spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            canceller.cancel();
        });
        // log watcher
        let watcher_handle = scope.spawn(move || {
            let mut sink_counter = 0;
            for msg in watcher {
                println!("Feedback message from the pipeline: {:?}", msg);
                if msg.message.contains("dummy sink") {
                    sink_counter += 1;
                }
            }
            sink_counter
        });
        let sink_counter = watcher_handle.join().unwrap();
        assert!(sink_counter > 10); // sink should receive more than 10 objects
        assert!(sink_counter < 80); // pipeline should be canceled before 50
    });

    // wait for the pipeline to finish
    handle.join();
}
