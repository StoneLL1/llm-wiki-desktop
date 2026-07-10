mod artifacts;
mod classification;
mod confirmation;
mod preview;
mod promotion;
mod source_actions;
mod source_catalog;

#[cfg(test)]
mod test_support;

pub use classification::classify_file;

#[derive(Default)]
pub struct ImportService;
