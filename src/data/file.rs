use crate::{
    data::arbs::{ArbDb, ArbFilterParams, WriteEngine},
    info,
    interfaces::{SimArbResultBatch, StoredArbsRanges},
    Result,
};
use async_trait::async_trait;
use std::{
    fs::File,
    io::{BufWriter, Write},
};

pub const EXPORT_DIR: &'static str = "./arbData";

fn parse_filename(filename: Option<String>) -> Result<String> {
    let filename = filename.unwrap_or(format!(
        "arbs_{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
    ));
    Ok(if filename.ends_with(".json") {
        filename.to_owned()
    } else {
        format!("{}.json", filename)
    })
}

#[derive(Clone, Debug)]
pub struct FileWriter {
    pub filename: String,
}

impl FileWriter {
    pub fn new(filename: Option<String>) -> Self {
        return FileWriter {
            filename: parse_filename(filename).expect("failed to parse filename"),
        };
    }

    pub async fn save_arbs_to_file(&self, arbs: &Vec<SimArbResultBatch>) -> Result<()> {
        // create EXPORT_DIR if it doesn't exist
        tokio::fs::create_dir_all(EXPORT_DIR).await?;
        let filename = format!("{}/{}", EXPORT_DIR, self.filename);
        if arbs.len() > 0 {
            info!("exporting {} arbs to file {}...", arbs.len(), filename);
            let file = File::options()
                .append(true)
                .create(true)
                .open(filename.to_owned())?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, &arbs)?;
            writer.flush()?;
        } else {
            info!("no arbs found to export.");
        }
        Ok(())
    }
}

#[async_trait]
impl ArbDb for FileWriter {
    /// Write arbs to a file.
    async fn write_arbs(&self, arbs: &Vec<SimArbResultBatch>) -> Result<()> {
        self.save_arbs_to_file(arbs).await
    }

    /* The following aren't really needed, but the trait requires them. Maybe I should break up the trait a bit.
    (TODO: try breaking ArbDb trait into ArbReader and ArbWriter)
    */
    async fn read_arbs(
        &self,
        _filter_params: &ArbFilterParams,
        _offset: Option<u64>,
        _limit: Option<i64>,
    ) -> Result<Vec<SimArbResultBatch>> {
        unimplemented!()
    }
    async fn get_num_arbs(&self, _filter_params: &ArbFilterParams) -> Result<u64> {
        unimplemented!()
    }
    async fn get_previously_saved_ranges(&self) -> Result<StoredArbsRanges> {
        unimplemented!()
    }
    async fn export_arbs(
        &self,
        _write_dest: WriteEngine,
        _filter_params: &ArbFilterParams,
    ) -> Result<()> {
        unimplemented!()
    }
}
