use anyhow::Result;
use std::path::PathBuf;

use crate::node_filter::{builtin_filters, NodeFilter};
use crate::structured_modes::{builtin_structured_modes, StructuredMode};

/// Persistent data structure that holds user-defined display modes and node filters.
/// If the data structure changes, it should be versioned to maintain compatibility with data saved
/// using older versions of traviz.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PersistentData {
    V1(PersistentDataV1),
}

impl Default for PersistentData {
    fn default() -> Self {
        PersistentData::V1(PersistentDataV1::default())
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PersistentDataV1 {
    display_modes: Vec<StructuredMode>,
    node_filters: Vec<NodeFilter>,
}

pub fn save_persistent_data(
    display_modes: &Vec<StructuredMode>,
    node_filters: &Vec<NodeFilter>,
) -> Result<()> {
    let mut dmodes = display_modes.clone();
    dmodes.retain(|mode| !mode.is_builtin);

    let mut filters = node_filters.clone();
    filters.retain(|filter| !filter.is_builtin);

    let data = PersistentData::V1(PersistentDataV1 {
        display_modes: dmodes,
        node_filters: filters,
    });

    write_data(&data)
}

pub fn load_persistent_data(
    display_modes: &mut Vec<StructuredMode>,
    node_filters: &mut Vec<NodeFilter>,
) -> Result<()> {
    let data = read_data()?;
    let (modes, filters) = match data {
        PersistentData::V1(data) => (data.display_modes, data.node_filters),
    };

    // Add builtin modes and filters which are not saved in persistent data
    *display_modes = builtin_structured_modes()
        .into_iter()
        .chain(modes.into_iter())
        .collect();

    *node_filters = builtin_filters()
        .into_iter()
        .chain(filters.into_iter())
        .collect();

    Ok(())
}

fn write_data(data: &PersistentData) -> Result<()> {
    let persistent_data_file = persistent_data_file_path();
    println!(
        "Writing persistent data to {}",
        persistent_data_file.display()
    );

    // Create the directory if it doesn't exist
    std::fs::create_dir_all(persistent_data_folder())?;

    // First write the data to a temporary file
    let write_file_path = temporary_write_file_path();
    let mut file = std::fs::File::create(&write_file_path)?;
    serde_json::to_writer_pretty(&mut file, &data)?;
    file.sync_all()?;

    // Then move the temporary file to the final location
    // Makes things more robust against crashes
    std::fs::rename(&write_file_path, persistent_data_file_path())?;

    Ok(())
}

fn read_data() -> Result<PersistentData> {
    let path = persistent_data_file_path();
    println!("Readng persistent data from {}", path.display());
    if !path.try_exists()? {
        println!("File not found, using default data");
        return Ok(PersistentData::default());
    }
    let file = std::fs::File::open(&path)?;
    let data: PersistentData = serde_json::from_reader(file)?;
    Ok(data)
}

fn persistent_data_folder() -> PathBuf {
    directories::ProjectDirs::from("org", "near", "traviz")
        .unwrap()
        .data_dir()
        .to_path_buf()
}

fn persistent_data_file_path() -> PathBuf {
    persistent_data_folder().join("persistent_data.json")
}

fn temporary_write_file_path() -> PathBuf {
    let random_number: u64 = rand::random();
    persistent_data_folder().join(format!("temporary_persistent_data{}.json", random_number))
}
