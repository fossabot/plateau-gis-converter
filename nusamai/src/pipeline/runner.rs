use std::sync::mpsc::sync_channel;

use rayon::{prelude::*, ThreadPoolBuilder};

use super::{
    feedback::{watcher, Feedback, Watcher},
    Canceller,
};
use crate::pipeline::{Receiver, Transformer};
use crate::sink::DataSink;
use crate::source::DataSource;

const SOURCE_OUTPUT_CHANNEL_BOUND: usize = 10000;
const TRANSFORMER_OUTPUT_CHANNEL_BOUND: usize = 10000;

fn run_source_thread(
    mut source: Box<dyn DataSource>,
    feedback: Feedback,
) -> (std::thread::JoinHandle<()>, Receiver) {
    let (sender, receiver) = sync_channel(SOURCE_OUTPUT_CHANNEL_BOUND);
    let handle = std::thread::spawn(move || {
        log::info!("Source thread started.");
        let num_threads = std::thread::available_parallelism()
            .map(|v| v.get() * 5)
            .unwrap_or(1);
        let pool = ThreadPoolBuilder::new()
            .use_current_thread()
            .num_threads(num_threads)
            .build()
            .unwrap();
        pool.install(|| {
            source.run(sender, &feedback);
        });
        log::info!("Source thread finished.");
    });
    (handle, receiver)
}

fn run_transformer_thread(
    transformer: Box<dyn Transformer>,
    upstream: Receiver,
    feedback: Feedback,
) -> (std::thread::JoinHandle<()>, Receiver) {
    let (sender, receiver) = sync_channel(TRANSFORMER_OUTPUT_CHANNEL_BOUND);
    let handle = std::thread::spawn(move || {
        let pool = ThreadPoolBuilder::new()
            .use_current_thread()
            .build()
            .unwrap();
        pool.install(|| {
            log::info!("Transformer threads started.");
            let _ = upstream.into_iter().par_bridge().try_for_each(|parcel| {
                if feedback.is_cancelled() {
                    log::info!("transformer cancelled");
                    return Err(());
                }
                if transformer.transform(parcel, &sender, &feedback).is_err() {
                    Err(())
                } else {
                    Ok(())
                }
            });
            log::info!("Transformer threads finished.");
        });
    });
    (handle, receiver)
}

fn run_sink_thread(
    mut sink: Box<dyn DataSink>,
    upstream: Receiver,
    mut feedback: Feedback,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        log::info!("Sink thread started.");
        let pool = ThreadPoolBuilder::new()
            .use_current_thread()
            .build()
            .unwrap();
        pool.install(|| {
            sink.run(upstream, &mut feedback);
        });
        log::info!("Sink thread finished.");
    })
}

pub struct PipelineHandle {
    thread_handles: Vec<std::thread::JoinHandle<()>>,
}

impl PipelineHandle {
    // Wait for the pipeline to terminate
    pub fn join(self) {
        self.thread_handles.into_iter().for_each(|handle| {
            if let Err(err) = handle.join() {
                log::error!("Error: {:#?}", err);
            }
        });
    }
}

/// Run the pipeline
///
/// `[Source] ==> [Transformer] ==> [Sink]`
pub fn run(
    source: Box<dyn DataSource>,
    transformer: Box<dyn Transformer>,
    sink: Box<dyn DataSink>,
) -> (PipelineHandle, Watcher, Canceller) {
    let (watcher, feedback, canceller) = watcher();

    // Start the pipeline
    let (source_thread, source_receiver) = run_source_thread(source, feedback.clone());
    let (transformer_thread, transformer_receiver) =
        run_transformer_thread(transformer, source_receiver, feedback.clone());
    let sink_thread = run_sink_thread(sink, transformer_receiver, feedback.clone());

    let handle = PipelineHandle {
        thread_handles: vec![source_thread, transformer_thread, sink_thread],
    };
    (handle, watcher, canceller)
}
