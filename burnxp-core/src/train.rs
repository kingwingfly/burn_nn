use burn::{
    data::dataloader::DataLoaderBuilder,
    lr_scheduler::linear::LinearLrSchedulerConfig,
    optim::AdamConfig,
    prelude::*,
    record::{CompactRecorder, Recorder},
    tensor::backend::AutodiffBackend,
    train::{
        metric::{
            store::{Aggregate, Direction, Split},
            CpuMemory, CpuUse, CudaMetric, HammingScore, LearningRateMetric, LossMetric,
        },
        LearnerBuilder, MetricEarlyStoppingStrategy, StoppingCondition,
    },
};
use std::{fs::File, path::PathBuf};
use tagger::DataSetDesc;

use crate::{
    data::{ImageBatcher, ImageDataSet},
    model::ModelConfig,
};

#[derive(Config)]
pub struct TrainingConfig {
    model: ModelConfig,
    optimizer: AdamConfig,
    train_set: PathBuf,
    valid_set: PathBuf,
    pretrained: Option<PathBuf>,
    #[config(default = false)]
    download_pretrained: bool,
    #[config(default = 64)]
    num_epochs: usize,
    #[config(default = 1)]
    batch_size: usize,
    #[config(default = 1)]
    num_workers: usize,
    #[config(default = 42)]
    seed: u64,
    #[cfg_attr(feature = "f16", config(default = 1.0e-6))]
    #[cfg_attr(not(feature = "f16"), config(default = 1.0e-4))]
    learning_rate: f64,
    #[config(default = 10)]
    early_stopping: usize,
    #[config(default = "0.5")]
    confidence_threshold: f32,
}

fn create_artifact_dir(artifact_dir: &PathBuf) {
    // Remove existing artifacts before to get an accurate learner summary
    std::fs::remove_dir_all(artifact_dir).ok();
    std::fs::create_dir_all(artifact_dir).ok();
}

pub fn train<B: AutodiffBackend>(
    artifact_dir: PathBuf,
    config: TrainingConfig,
    devices: Vec<B::Device>,
) {
    create_artifact_dir(&artifact_dir);
    #[cfg(not(feature = "candle"))]
    B::seed(config.seed);

    config
        .save(artifact_dir.join("train_config.json"))
        .expect("Config should be saved successfully");

    let mut train_input: DataSetDesc = serde_json::from_reader(
        File::open(config.train_set).expect("Train set file should be accessible"),
    )
    .expect("Train set file should be legal");
    let loss_weights = train_input.loss_weights.take();
    let valid_input: DataSetDesc = serde_json::from_reader(
        File::open(config.valid_set).expect("Validation set file should be accessible"),
    )
    .expect("Validation set file should be legal");

    let num_classes = train_input.num_classes;
    let dataset_train = ImageDataSet::train(train_input).expect("Training set failed to be loaded");
    let num_iters = dataset_train.len / config.batch_size * config.early_stopping;
    let dataloader_train = DataLoaderBuilder::new(ImageBatcher::new())
        .batch_size(config.batch_size)
        .shuffle(config.seed)
        .num_workers(config.num_workers)
        .build(dataset_train);

    let dataloader_valid = DataLoaderBuilder::new(ImageBatcher::new())
        .batch_size(config.batch_size)
        .shuffle(config.seed)
        .num_workers(config.num_workers)
        .build(ImageDataSet::valid(valid_input).expect("Validation set faild to be loaded"));

    let learner = LearnerBuilder::new(&artifact_dir)
        .metric_train_numeric(HammingScore::new().with_threshold(config.confidence_threshold))
        .metric_valid_numeric(HammingScore::new().with_threshold(config.confidence_threshold))
        .metric_train_numeric(LossMetric::new())
        .metric_train(LearningRateMetric::new())
        .metric_train(CudaMetric::new())
        .metric_train(CpuUse::new())
        .metric_train(CpuMemory::new())
        .early_stopping(MetricEarlyStoppingStrategy::new::<HammingScore<B>>(
            Aggregate::Mean,
            Direction::Highest,
            Split::Valid,
            StoppingCondition::NoImprovementSince { n_epochs: config.early_stopping },
        ))
        .with_file_checkpointer(CompactRecorder::new())
        .devices(devices.clone())
        .num_epochs(config.num_epochs)
        .summary()
        .build(
            {
                let mut model = config.model.with_loss_weights(loss_weights).with_download(config.download_pretrained).init::<B>(&devices[0], num_classes);
                if let Some(pretrain) = config.pretrained {
                    model = model.load_record(
                        CompactRecorder::new()
                            .load(pretrain, &devices[0])
                            .expect("Please offer a valid pretrained model. If it's in the artifact directory, it is removed when recreating the directory."),
                    )
                }
                model
            },
            config.optimizer.init(),
            LinearLrSchedulerConfig::new(config.learning_rate, config.learning_rate/10., num_iters ).init().unwrap(),
        );

    let model_trained = learner.fit(dataloader_train, dataloader_valid);

    model_trained
        .save_file(artifact_dir.join("model"), &CompactRecorder::new())
        .expect("Trained model should be saved successfully");
}
