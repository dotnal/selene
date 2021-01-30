pub mod client;
mod extractor;

pub struct Lesson {
    _no: usize,
    link: String,
}

pub struct LessonPart {
    url: String,
}
