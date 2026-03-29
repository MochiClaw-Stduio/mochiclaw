//! Lambda discovery - scans directories for lambdas

use crate::manifest::LambdaManifest;
use std::path::{Path, PathBuf};

/// Discovered lambda info
#[derive(Debug)]
pub struct DiscoveredLambda {
    pub manifest: LambdaManifest,
    pub wasm_path: PathBuf,
}

/// Discover lambdas from a directory
///
/// Scans subdirectories of the given path, looking for:
/// - A `manifest.toml` file to read lambda metadata
/// - A corresponding `.wasm` file in the target directory
///
/// Returns a list of discovered lambdas that have `features.provider` or `features.channel` set.
pub fn discover(lambda_dir: &Path) -> Result<Vec<DiscoveredLambda>, std::io::Error> {
    let mut lambdas = Vec::new();

    if !lambda_dir.exists() {
        tracing::warn!("lambda directory does not exist: {}", lambda_dir.display());
        return Ok(lambdas);
    }

    // Scan subdirectories for lambdas
    for entry in std::fs::read_dir(lambda_dir)? {
        let entry = entry?;
        let entry_path = entry.path();

        if !entry_path.is_dir() {
            continue;
        }

        // Check for manifest.toml
        let manifest_path = entry_path.join("manifest.toml");
        let manifest = match LambdaManifest::from_file(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    "failed to load manifest at '{}': {}",
                    manifest_path.display(),
                    e
                );
                continue;
            }
        };

        // Check if this lambda should be loaded
        if !manifest.features.provider
            && !manifest.features.channel
            && !manifest.features.command
            && !manifest.features.tool
        {
            tracing::debug!(
                "skipping '{}': not a channel, provider, command, or tool",
                manifest.name
            );
            continue;
        }

        // Find the wasm file (same directory as manifest.toml, named after manifest.name)
        let wasm_path = entry_path.join(format!("{}.wasm", manifest.name));

        if !wasm_path.exists() {
            tracing::debug!(
                "skipping '{}': no wasm found at {}",
                manifest.name,
                wasm_path.display()
            );
            continue;
        }

        lambdas.push(DiscoveredLambda {
            manifest,
            wasm_path,
        });
    }

    Ok(lambdas)
}
