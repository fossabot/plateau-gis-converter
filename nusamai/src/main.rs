use std::env;
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

use clap::Parser;

use nusamai::pipeline::Canceller;

use nusamai::sink::{DataRequirements, DataSink, DataSinkProvider};
use nusamai::source::citygml::CityGmlSourceProvider;
use nusamai::source::{DataSource, DataSourceProvider};
use nusamai::transformer::MultiThreadTransformer;
use nusamai::transformer::{self, MappingRules};
use nusamai::transformer::{NusamaiTransformBuilder, TransformBuilder};
use nusamai::BUILTIN_SINKS;
use nusamai_citygml::CityGmlElement;
use nusamai_plateau::models::TopLevelCityObject;

#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Specify path patterns to the input CityGML files
    #[arg()]
    file_patterns: Vec<String>,

    /// Select the output format
    #[arg(value_enum, long)]
    sink: SinkChoice,

    /// Specify the output path
    #[arg(long)]
    output: String,

    /// Specify the mapping rules JSON file
    #[arg(long)]
    rules: Option<String>,

    /// Output schema
    #[arg(long)]
    schema: Option<String>,

    /// Add an option for the output format (key=value)
    #[arg(short = 'o', value_parser = parse_key_val)]
    sinkopt: Vec<(String, String)>,

    /// Add an option for the input source (key=value)
    #[arg(short = 'i', value_parser = parse_key_val)]
    sourceopt: Vec<(String, String)>,
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].into(), s[pos + 1..].into()))
}

#[derive(Clone)]
struct SinkChoice(String);

static SINK_CHOICE_VARIANTS: OnceLock<Vec<SinkChoice>> = OnceLock::new();

impl clap::ValueEnum for SinkChoice {
    fn value_variants<'a>() -> &'a [Self] {
        SINK_CHOICE_VARIANTS.get_or_init(|| {
            BUILTIN_SINKS
                .iter()
                .map(|provider| Self(provider.info().id_name))
                .collect()
        });
        SINK_CHOICE_VARIANTS.get().unwrap()
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        BUILTIN_SINKS
            .iter()
            .find(|provider| provider.info().id_name == self.0)
            .map(|provider| {
                let info = provider.info();
                clap::builder::PossibleValue::new(info.id_name).help(info.name)
            })
    }
}

impl SinkChoice {
    fn create_sink(&self) -> &dyn DataSinkProvider {
        for &provider in nusamai::BUILTIN_SINKS {
            if self.0 == provider.info().id_name {
                return provider;
            }
        }
        panic!("Unknown sink choice: {:?}", self.0);
    }
}

fn main() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info")
    }
    pretty_env_logger::init();

    let args = {
        // output path
        let mut args = Args::parse();
        args.sinkopt.push(("@output".into(), args.output.clone()));
        args
    };

    let mut canceller = Arc::new(Mutex::new(Canceller::default()));
    {
        let canceller = canceller.clone();
        ctrlc::set_handler(move || {
            log::info!("request cancellation");
            canceller.lock().unwrap().cancel();
        })
        .expect("Error setting Ctrl-C handler");
    }

    let sink = {
        let sink_provider = args.sink.create_sink();
        let mut sink_params = sink_provider.parameters();
        if let Err(err) = sink_params.update_values_with_str(&args.sinkopt) {
            log::error!("Error parsing sink options: {:?}", err);
            return;
        };
        if let Err(err) = sink_params.validate() {
            log::error!("Error validating source parameters: {:?}", err);
            return;
        }
        sink_provider.create(&sink_params)
    };

    let requirements = sink.make_requirements();

    let mapping_rules = match &args.rules {
        Some(rules_path) => {
            let Ok(file_contents) = std::fs::read_to_string(rules_path) else {
                log::error!("Error reading rules file: {}", rules_path);
                return;
            };
            let Ok(mapping_rules) = serde_json::from_str::<MappingRules>(&file_contents) else {
                log::error!("Error parsing rules file");
                return;
            };
            Some(mapping_rules)
        }
        None => None,
    };

    let source = {
        // glob input file patterns
        let mut filenames = vec![];
        for file_pattern in &args.file_patterns {
            let file_pattern = shellexpand::tilde(file_pattern);
            for entry in glob::glob(&file_pattern).unwrap() {
                filenames.push(entry.unwrap());
            }
        }

        let source_provider: Box<dyn DataSourceProvider> =
            Box::new(CityGmlSourceProvider { filenames });
        let mut source_params = source_provider.parameters();
        if let Err(err) = source_params.update_values_with_str(&args.sourceopt) {
            log::error!("Error parsing source parameters: {:?}", err);
            return;
        };
        if let Err(err) = source_params.validate() {
            log::error!("Error validating source parameters: {:?}", err);
            return;
        }

        // create source
        let mut source = source_provider.create(&source_params);
        source.set_appearance_parsing(requirements.use_appearance);
        source
    };

    run(
        &args,
        source,
        requirements,
        mapping_rules,
        sink,
        &mut canceller,
    );
}

fn run(
    args: &Args,
    source: Box<dyn DataSource>,
    requirements: DataRequirements,
    mapping_rules: Option<MappingRules>,
    sink: Box<dyn DataSink>,
    canceller: &mut Arc<Mutex<Canceller>>,
) {
    let total_time = std::time::Instant::now();

    // Prepare the transformer for the pipeline and transform the schema
    let (transformer, schema) = {
        let request = {
            let mut request = transformer::Request::from(requirements);
            request.set_mapping_rules(mapping_rules);
            request
        };
        let transform_builder = NusamaiTransformBuilder::new(request);
        let mut schema = nusamai_citygml::schema::Schema::default();
        TopLevelCityObject::collect_schema(&mut schema);
        transform_builder.transform_schema(&mut schema);

        if let Some(schema_path) = &args.schema {
            // TODO: error handling
            let mut file = std::fs::File::create(schema_path).unwrap();
            file.write_all(serde_json::to_string_pretty(&schema).unwrap().as_bytes())
                .unwrap();
        }

        let transformer = Box::new(MultiThreadTransformer::new(transform_builder));
        (transformer, schema)
    };

    // start the pipeline
    let (handle, watcher, inner_canceller) =
        nusamai::pipeline::run(source, transformer, sink, schema.into());
    *canceller.lock().unwrap() = inner_canceller;

    std::thread::scope(|scope| {
        // log watcher
        scope.spawn(move || {
            for msg in watcher {
                log::info!("Feedback message from the pipeline {:?}", msg);
            }
        });
    });

    // wait for the pipeline to finish
    handle.join();
    if canceller.lock().unwrap().is_canceled() {
        log::info!("Pipeline canceled");
    }

    log::info!("Total processing time: {:?}", total_time.elapsed());
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_run_cmd() {
        use assert_cmd::Command;

        let mut cmd = Command::cargo_bin("nusamai").unwrap();
        let assert = cmd
            .arg("../../nusamai-plateau/tests/data/sendai-shi/udx/urf/574026_urf_6668_huchi_op.gml")
            .arg("--sink")
            .arg("noop")
            .arg("--output")
            .arg("dummy")
            .arg("--rules")
            .arg("./tests/rules.json")
            .arg("--schema")
            .arg("schema.json")
            .assert();
        assert.success();
    }
}
