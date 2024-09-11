use burn::{
    data::dataloader::{batcher::Batcher, Dataset},
    prelude::*,
};
use image::{imageops::FilterType, ImageReader};
use std::fs::File;
use std::path::PathBuf;

type Score = f32;

#[derive(Debug, Clone)]
pub(crate) struct ImageData {
    data: [u8; 3 * 1024 * 1024],
    score: f32,
}

impl ImageData {
    pub(crate) fn data<B: Backend>(&self) -> Tensor<B, 3> {
        Tensor::from_data(self.data, &B::Device::default())
    }

    pub(crate) fn score<B: Backend>(&self) -> Tensor<B, 1> {
        Tensor::from_data([self.score], &B::Device::default())
    }
}

pub(crate) struct ImageDataSet {
    inner: Vec<(PathBuf, Score)>,
}

impl ImageDataSet {
    pub(crate) fn train(path: PathBuf) -> Self {
        Self {
            inner: serde_json::from_reader(File::open(path).unwrap()).unwrap(),
        }
    }

    pub(crate) fn test(path: PathBuf) -> Self {
        Self {
            inner: serde_json::from_reader(File::open(path).unwrap()).unwrap(),
        }
    }
}

impl Dataset<ImageData> for ImageDataSet {
    fn get(&self, index: usize) -> Option<ImageData> {
        self.inner.get(index).and_then(|(path, score)| {
            Some(ImageData {
                data: open_image(path)?,
                score: *score,
            })
        })
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

#[derive(Clone)]
pub(crate) struct PicBatcher<B: Backend> {
    device: B::Device,
}

#[derive(Debug, Clone)]
pub(crate) struct ImageBatch<B: Backend> {
    pub datas: Tensor<B, 4>,
    pub target_scores: Tensor<B, 2>,
}

impl<B: Backend> PicBatcher<B> {
    pub(crate) fn new(device: B::Device) -> Self {
        Self { device }
    }
}

impl<B: Backend> Batcher<ImageData, ImageBatch<B>> for PicBatcher<B> {
    fn batch(&self, items: Vec<ImageData>) -> ImageBatch<B> {
        let datas = items
            .iter()
            .map(|item| item.data().reshape([1, 3, 1024, 1024]))
            .collect();
        let target_scores = items
            .iter()
            .map(|item| item.score().reshape([1, 1]))
            .collect();

        let datas = Tensor::cat(datas, 0).to_device(&self.device);
        let target_scores = Tensor::cat(target_scores, 0).to_device(&self.device);

        ImageBatch {
            datas,
            target_scores,
        }
    }
}

fn open_image(path: &PathBuf) -> Option<[u8; 3 * 1024 * 1024]> {
    let img = ImageReader::open(path).ok()?.decode().ok()?;
    let mut background = image::RgbImage::new(1024, 1024);

    let factor = img.height().max(img.width()) / 1024;
    let new_width = (img.width() / factor) as i64;
    let new_height = (img.height() / factor) as i64;

    let img = img.resize(new_width as u32, new_height as u32, FilterType::Nearest);
    let img = img.to_rgb8();

    image::imageops::overlay(
        &mut background,
        &img,
        512 - new_width / 2,
        512 - new_height / 2,
    );

    background.into_raw().try_into().ok()
}
