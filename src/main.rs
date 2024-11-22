use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use glob::glob;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
struct ModelLayer {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct ModelManifest {
    layers: Vec<ModelLayer>,
}

#[derive(Debug)]
struct ModelUsage {
    name: String,
    last_used: DateTime<Local>,
    usage_count: usize,
    size: u64,
}

fn get_model_dir() -> PathBuf {
    if let Ok(custom_path) = env::var("OLLAMA_MODELS") {
        return PathBuf::from(custom_path);
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap()
            .join(".ollama")
            .join("models")
    }

    #[cfg(target_os = "windows")]
    {
        dirs::home_dir()
            .unwrap()
            .join(".ollama")
    }

    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/usr/share/ollama")
    }
}

fn get_log_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut paths: Vec<_> = glob(
            dirs::home_dir()
                .unwrap()
                .join(".ollama")
                .join("logs")
                .join("server*.log")
                .to_str()
                .unwrap(),
        )
        .unwrap()
        .filter_map(Result::ok)
        .collect();
        
        paths.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        paths
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = dirs::data_local_dir() {
            vec![local_app_data.join("Ollama")]
        } else {
            vec![]
        }
    }

    #[cfg(target_os = "linux")]
    {
        vec![]
    }
}

fn parse_manifest_path(path: &Path) -> Option<String> {
    let components: Vec<_> = path.components().collect();
    let len = components.len();
    if len >= 4 {
        let _registry = components[len - 4].as_os_str().to_string_lossy();
        let user = components[len - 3].as_os_str().to_string_lossy();
        let model = components[len - 2].as_os_str().to_string_lossy();
        let tag = path.file_name()?.to_string_lossy();

        let prefix = if user == "library" {
            String::new()
        } else {
            format!("{}/", user)
        };

        Some(format!("{}{}:{}", prefix, model, tag))
    } else {
        None
    }
}

fn find_model_manifests() -> Result<HashMap<String, (String, u64)>> {
    let mut hash_to_name_size = HashMap::new();

    let model_dir = get_model_dir();
    let manifest_dir = model_dir.join("manifests");

    for entry in glob(&format!("{}/**/*", manifest_dir.display()))
        .context("Failed to read glob pattern")?
    {
        let path = entry.context("Failed to get manifest path")?;
        if path.is_file() {
            let content = fs::read_to_string(&path).context("Failed to read manifest file")?;
            if let Ok(manifest) = serde_json::from_str::<ModelManifest>(&content) {
                if let Some(model_layer) = manifest
                    .layers
                    .iter()
                    .find(|l| l.media_type == "application/vnd.ollama.image.model")
                {
                    let hash = model_layer
                        .digest
                        .strip_prefix("sha256:")
                        .unwrap_or(&model_layer.digest)
                        .to_string();
                    
                    if let Some(model_name) = parse_manifest_path(&path) {
                        let entry = hash_to_name_size.entry(hash).or_insert_with(|| (String::new(), 0));
                        if !entry.0.is_empty() {
                            entry.0.push_str(", ");
                        }
                        entry.0.push_str(&model_name);
                        entry.1 = model_layer.size;
                    }
                }
            }
        }
    }

    Ok(hash_to_name_size)
}

fn parse_logs(hash_to_name_size: &HashMap<String, (String, u64)>) -> Result<HashMap<String, ModelUsage>> {
    let mut model_usage = HashMap::new();
    let log_paths = get_log_paths();
    let mut seen_hashes = HashSet::new();

    for log_path in log_paths {
        let file = File::open(&log_path)?;
        let metadata = file.metadata()?;
        let file_time = metadata.modified()?.into();
        
        let reader = BufReader::new(file);
        let mut last_timestamp: Option<DateTime<Local>> = None;

        for line in reader.lines() {
            let line = line?;
            if line.starts_with("time=") {
                if let Ok(timestamp) = DateTime::parse_from_rfc3339(&line[5..]) {
                    last_timestamp = Some(timestamp.with_timezone(&Local));
                }
            } else if line.len() > 19 && &line[4..5] == "/" && &line[7..8] == "/" {
                if let Ok(naive) = NaiveDateTime::parse_from_str(&line[0..19], "%Y/%m/%d %H:%M:%S") {
                    last_timestamp = Some(Local.from_local_datetime(&naive).unwrap());
                }
            } else if line.starts_with("llama_model_loader: loaded meta data") {
                if let Some(hash_start) = line.find("sha256-") {
                    let hash = line[hash_start + 7..hash_start + 71].to_string();
                    seen_hashes.insert(hash.clone());
                    
                    let (model_name, size) = hash_to_name_size
                        .get(&hash)
                        .map(|(name, size)| (name.clone(), *size))
                        .unwrap_or_else(|| (format!("{}...-deleted", &hash[..8]), 0));

                    let entry = model_usage.entry(model_name.clone()).or_insert_with(|| ModelUsage {
                        name: model_name,
                        last_used: last_timestamp.unwrap_or(file_time),
                        usage_count: 0,
                        size,
                    });

                    entry.usage_count += 1;
                    if let Some(timestamp) = last_timestamp {
                        if timestamp > entry.last_used {
                            entry.last_used = timestamp;
                        }
                    }
                }
            }
        }
    }

    Ok(model_usage)
}

fn main() -> Result<()> {
    let hash_to_name_size = find_model_manifests()?;
    let model_usage = parse_logs(&hash_to_name_size)?;

    // Split models into active and deleted
    let mut active_models: Vec<_> = model_usage.values()
        .filter(|m| !m.name.ends_with("-deleted"))
        .collect();
    let mut deleted_models: Vec<_> = model_usage.values()
        .filter(|m| m.name.ends_with("-deleted"))
        .collect();

    // Sort both lists by last used time (primary) and usage count (secondary)
    for models in [&mut active_models, &mut deleted_models] {
        models.sort_by(|a, b| {
            b.last_used
                .cmp(&a.last_used)
                .then_with(|| b.usage_count.cmp(&a.usage_count))
        });
    }

    // Get unlogged models
    let mut unlogged_models: Vec<_> = hash_to_name_size
        .values()
        .flat_map(|(name, size)| name.split(", ").map(move |n| (n, *size)))
        .filter(|(name, _)| !model_usage.values().any(|m| {
            // Split the model usage name in case it's a combined name
            m.name.split(", ").any(|usage_name| usage_name == *name)
        }))
        .collect();
    unlogged_models.sort_by(|a, b| a.0.cmp(&b.0));

    // Helper function to format size in GB or MB
    let format_size = |size: u64| -> String {
        let gb = size as f64 / 1_024.0 / 1_024.0 / 1_024.0;
        if gb >= 1.0 {
            format!("{:.1} GB", gb)
        } else {
            let mb = size as f64 / 1_024.0 / 1_024.0;
            format!("{:.1} MB", mb)
        }
    };

    // Helper function to print a table
    let print_table = |models: &[&ModelUsage], title: &str| {
        if models.is_empty() {
            return;
        }

        let is_deleted = models.iter().any(|m| m.name.ends_with("-deleted"));
        let is_unlogged = models.iter().all(|m| m.usage_count == 0 && m.last_used == Local::now());

        // Calculate column widths
        let model_width = "Model".len().max(
            models
                .iter()
                .map(|m| m.name.len())
                .max()
                .unwrap_or(0)
        );

        let (last_used_width, usage_count_width) = if !is_unlogged {
            (
                "Last Used".len().max(10),  // YYYY-MM-DD is 10 chars
                "Usage Count".len().max(
                    models
                        .iter()
                        .map(|m| m.usage_count.to_string().len())
                        .max()
                        .unwrap_or(0)
                )
            )
        } else {
            (0, 0)
        };

        let show_size = !is_deleted;
        let size_width = if show_size {
            "Size".len().max(
                models
                    .iter()
                    .map(|m| format_size(m.size).len())
                    .max()
                    .unwrap_or(0)
            )
        } else {
            0
        };

        // Print title and header
        println!("\n{}", title);
        if is_unlogged {
            println!("{:width$}  {:>size_width$}",
                "Model",
                "Size",
                width = model_width,
                size_width = size_width
            );

            // Print separator
            println!("{:-<width$}  {:-<size_width$}",
                "",
                "",
                width = model_width,
                size_width = size_width
            );

            // Print data rows
            for usage in models {
                println!("{:width$}  {:>size_width$}",
                    usage.name,
                    format_size(usage.size),
                    width = model_width,
                    size_width = size_width
                );
            }
        } else if show_size {
            println!("{:width$}  {:last_used_width$}  {:>usage_count_width$}  {:>size_width$}",
                "Model",
                "Last Used",
                "Usage Count",
                "Size",
                width = model_width,
                last_used_width = last_used_width,
                usage_count_width = usage_count_width,
                size_width = size_width
            );

            // Print separator
            println!("{:-<width$}  {:-<last_used_width$}  {:-<usage_count_width$}  {:-<size_width$}",
                "",
                "",
                "",
                "",
                width = model_width,
                last_used_width = last_used_width,
                usage_count_width = usage_count_width,
                size_width = size_width
            );

            // Print data rows
            for usage in models {
                println!("{:width$}  {:last_used_width$}  {:>usage_count_width$}  {:>size_width$}",
                    usage.name,
                    usage.last_used.format("%Y-%m-%d"),
                    usage.usage_count,
                    format_size(usage.size),
                    width = model_width,
                    last_used_width = last_used_width,
                    usage_count_width = usage_count_width,
                    size_width = size_width
                );
            }
        } else {
            println!("{:width$}  {:last_used_width$}  {:>usage_count_width$}",
                "Model",
                "Last Used",
                "Usage Count",
                width = model_width,
                last_used_width = last_used_width,
                usage_count_width = usage_count_width
            );

            // Print separator
            println!("{:-<width$}  {:-<last_used_width$}  {:-<usage_count_width$}",
                "",
                "",
                "",
                width = model_width,
                last_used_width = last_used_width,
                usage_count_width = usage_count_width
            );

            // Print data rows
            for usage in models {
                println!("{:width$}  {:last_used_width$}  {:>usage_count_width$}",
                    usage.name,
                    usage.last_used.format("%Y-%m-%d"),
                    usage.usage_count,
                    width = model_width,
                    last_used_width = last_used_width,
                    usage_count_width = usage_count_width
                );
            }
        }
    };

    print_table(&active_models, "Active Models:");

    if !unlogged_models.is_empty() {
        println!("\nUnlogged Models:");
        println!("---------------");
        let model_width = unlogged_models.iter().map(|(name, _)| name.len()).max().unwrap_or(0).max("Model".len());
        let size_width = unlogged_models.iter().map(|(_, size)| format_size(*size).len()).max().unwrap_or(0).max("Size".len());

        // Print header
        println!("{:width$}  {:>size_width$}",
            "Model",
            "Size",
            width = model_width,
            size_width = size_width
        );

        // Print separator
        println!("{:-<width$}  {:-<size_width$}",
            "",
            "",
            width = model_width,
            size_width = size_width
        );

        // Print data rows
        for (name, size) in &unlogged_models {
            println!("{:width$}  {:>size_width$}",
                name,
                format_size(*size),
                width = model_width,
                size_width = size_width
            );
        }
    }

    print_table(&deleted_models, "Deleted Models:");
    println!();

    Ok(())
}
