use crate::elements::SlideElement;

#[derive(Debug)]
pub struct Presentation {
    pub slides: Vec<Slide>,
    pub metadata: PresentationMetadata,
}

#[derive(Debug, Default)]
pub struct PresentationMetadata {
    pub title: Option<String>,
}

#[derive(Debug)]
pub struct Slide {
    pub chunks: Vec<SlideChunk>,
    pub notes: Option<String>,
}

#[derive(Debug)]
pub struct SlideChunk {
    pub elements: Vec<SlideElement>,
}
