//! Reading one of `ctr`'s aligned tables.

use rastro_collector::CollectionError;

/// A table `ctr` printed, sliced by the offsets of its own header.
///
/// **Sliced rather than split, because one column holds two words.** `ctr images ls` prints
/// the size as `3.9 MiB`, so splitting a row on whitespace puts the platforms where the
/// labels belong and shifts every field after the size. The header is padded to the width of
/// the widest cell in each column, which makes its own column offsets the authority on where
/// each field starts, and a column `ctr` adds later shifts nothing that is read by name.
///
/// Used only where `ctr` offers nothing better. Every other read in this collector asks for
/// `--quiet` or JSON.
pub struct CtrTable {
    columns: Vec<(String, usize)>,
    rows: Vec<String>,
}

impl CtrTable {
    /// Reads the header and the rows under it, refusing output with no header at all.
    pub fn parse(output: &str, read: &str) -> Result<Self, CollectionError> {
        let mut lines = output.lines();
        let header = lines.next().ok_or_else(|| {
            CollectionError::new(format!(
                "could not read what `ctr {read}` reported: it printed nothing at all"
            ))
        })?;

        let columns = columns_of(header);
        if columns.is_empty() {
            return Err(CollectionError::new(format!(
                "could not read what `ctr {read}` reported: its first line names no columns"
            )));
        }

        Ok(Self {
            columns,
            rows: lines
                .filter(|row| !row.trim().is_empty())
                .map(str::to_owned)
                .collect(),
        })
    }

    /// Every row, as the named columns of it.
    ///
    /// A row shorter than a column's offset yields nothing for that column, which is what a
    /// trailing column left empty looks like once the padding is gone.
    pub fn rows(&self) -> Vec<Vec<(&str, String)>> {
        self.rows.iter().map(|row| self.cells_of(row)).collect()
    }

    fn cells_of(&self, row: &str) -> Vec<(&str, String)> {
        let mut cells = Vec::new();

        for (index, (name, start)) in self.columns.iter().enumerate() {
            let end = self
                .columns
                .get(index + 1)
                .map_or(row.len(), |(_, next)| (*next).min(row.len()));
            let start = (*start).min(row.len());

            cells.push((name.as_str(), row[start..end].trim().to_owned()));
        }

        cells
    }
}

/// The header's column names, each with the byte offset it starts at.
///
/// Byte offsets rather than character ones: `ctr`'s headers are ASCII, and a row's own cells
/// are sliced by the same offsets, so both sides agree by construction.
fn columns_of(header: &str) -> Vec<(String, usize)> {
    let mut columns = Vec::new();
    let mut start = None;

    for (offset, character) in header.char_indices() {
        match (character.is_whitespace(), start) {
            (false, None) => start = Some(offset),
            (true, Some(from)) => {
                columns.push((header[from..offset].to_owned(), from));
                start = None;
            }
            _ => {}
        }
    }

    if let Some(from) = start {
        columns.push((header[from..].to_owned(), from));
    }

    columns
}
