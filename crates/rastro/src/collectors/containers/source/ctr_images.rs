//! `ctr images ls`: which images a namespace holds.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, NonEmptyText};

use super::ctr_table::CtrTable;
use crate::collectors::containers::model::ContainerdImage;
use crate::collectors::containers::value_objects::{ImageDigest, ImageReference};

/// What the read is called in a failure, and the columns it reads by name.
const READ: &str = "images ls";
const REFERENCE: &str = "REF";
const MEDIA_TYPE: &str = "TYPE";
const DIGEST: &str = "DIGEST";
const PLATFORMS: &str = "PLATFORMS";

/// How the platforms of one image are joined in the cell.
const PLATFORM_SEPARATOR: char = ',';

/// The images of one namespace, keyed by reference.
///
/// **Read by column name from an aligned table**, because `ctr` prints no machine-readable
/// form of this and one of its columns holds two words. See [`CtrTable`].
///
/// The size and the labels `ctr` also prints are deliberately not read; the reasons are on
/// [`ContainerdImage`].
pub struct CtrImages;

impl CtrImages {
    pub fn parse(
        output: &str,
    ) -> Result<BTreeMap<ImageReference, ContainerdImage>, CollectionError> {
        let table = CtrTable::parse(output, READ)?;
        let mut images = BTreeMap::new();

        for row in table.rows() {
            let cell = |name: &str| {
                row.iter()
                    .find(|(column, _)| *column == name)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default()
            };

            let reference = ImageReference::new(cell(REFERENCE))?;
            let mut platforms = Vec::new();
            for platform in cell(PLATFORMS).split(PLATFORM_SEPARATOR) {
                if let Ok(platform) = NonEmptyText::new(platform.trim(), "image platform") {
                    platforms.push(platform);
                }
            }
            // Sorted, because the index carries them in its own order and promises none.
            platforms.sort();

            let image = ContainerdImage {
                media_type: NonEmptyText::new(cell(MEDIA_TYPE), "image media type")?,
                digest: ImageDigest::new(cell(DIGEST))?,
                platforms,
            };

            if images.insert(reference.clone(), image).is_some() {
                return Err(CollectionError::new(format!(
                    "containerd reported the image {:?} twice in one namespace, so the \
                     answer was misread",
                    reference.as_str()
                )));
            }
        }

        Ok(images)
    }
}
