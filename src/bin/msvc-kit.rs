//! msvc-kit CLI - Portable MSVC Build Tools installer and manager

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use msvc_kit::bundle::{generate_bundle_scripts, save_bundle_scripts, BundleLayout};
use msvc_kit::env::ShellType;
use msvc_kit::installer::{install_msvc, install_sdk};
use msvc_kit::version::{list_installed_msvc, list_installed_sdk, Architecture};
use msvc_kit::{
    download_msvc, download_sdk, generate_activation_script, generate_activation_script_with_vars,
    get_env_vars, load_config, save_config, setup_environment, DownloadOptions, MsvcKitConfig,
};

/// Portable MSVC Build Tools installer and manager
#[derive(Parser)]
#[command(name = "msvc-kit")]
#[command(author = "loonghao <hal.long@outlook.com>")]
#[command(version)]
#[command(about = "Download and manage MSVC compiler and Windows SDK", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Configuration file path
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download MSVC and/or Windows SDK components
    Download {
        /// MSVC version to download (default: latest)
        #[arg(long)]
        msvc_version: Option<String>,

        /// Windows SDK version to download (default: latest)
        #[arg(long)]
        sdk_version: Option<String>,

        /// Target directory for installation
        #[arg(short, long)]
        target: Option<PathBuf>,

        /// Target architecture (x64, x86, arm64)
        #[arg(short, long, default_value = "x64")]
        arch: String,

        /// Skip MSVC download
        #[arg(long)]
        no_msvc: bool,

        /// Skip Windows SDK download
        #[arg(long)]
        no_sdk: bool,

        /// Skip hash verification
        #[arg(long)]
        no_verify: bool,

        /// Max parallel downloads
        #[arg(long)]
        parallel_downloads: Option<usize>,
    },

    /// Setup environment variables for MSVC toolchain
    Setup {
        /// Installation directory (default: from config)
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// Target architecture
        #[arg(short, long, default_value = "x64")]
        arch: String,

        /// Generate activation script instead of modifying environment
        #[arg(long)]
        script: bool,

        /// Shell type for script (cmd, powershell, bash)
        #[arg(long, default_value = "powershell")]
        shell: String,

        /// Replace install root with a portable placeholder when generating scripts (requires --script)
        #[arg(long, requires = "script", value_name = "PORTABLE_ROOT")]
        portable_root: Option<String>,

        /// Write to Windows registry (persistent)
        #[arg(long)]
        persistent: bool,
    },

    /// List installed versions
    List {
        /// Installation directory
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// Show available versions from Microsoft
        #[arg(long)]
        available: bool,
    },

    /// Remove installed versions
    Clean {
        /// Installation directory
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// MSVC version to remove
        #[arg(long)]
        msvc_version: Option<String>,

        /// SDK version to remove
        #[arg(long)]
        sdk_version: Option<String>,

        /// Remove all installed versions
        #[arg(long)]
        all: bool,

        /// Also remove downloaded cache
        #[arg(long)]
        cache: bool,
    },

    /// Show current configuration
    Config {
        /// Set installation directory
        #[arg(long)]
        set_dir: Option<PathBuf>,

        /// Set default MSVC version
        #[arg(long)]
        set_msvc: Option<String>,

        /// Set default SDK version
        #[arg(long)]
        set_sdk: Option<String>,

        /// Reset configuration to defaults
        #[arg(long)]
        reset: bool,
    },

    /// Print environment variables for shell integration
    Env {
        /// Installation directory
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// Output format (shell, json)
        #[arg(short, long, default_value = "shell")]
        format: String,
    },

    /// Create a portable bundle with MSVC toolchain (downloads components locally)
    Bundle {
        /// Output directory for the bundle
        #[arg(short, long, default_value = "./msvc-bundle")]
        output: PathBuf,

        /// Target architecture (x64, x86, arm64)
        #[arg(short, long, default_value = "x64")]
        arch: String,

        /// MSVC version to download (default: latest)
        #[arg(long)]
        msvc_version: Option<String>,

        /// Windows SDK version to download (default: latest)
        #[arg(long)]
        sdk_version: Option<String>,

        /// Accept Microsoft license terms (required)
        #[arg(long)]
        accept_license: bool,

        /// Create a zip archive of the bundle
        #[arg(long)]
        zip: bool,
    },

    #[cfg(feature = "self-update")]
    /// Update msvc-kit to the latest version
    Update {
        /// Check for updates without installing
        #[arg(long)]
        check: bool,

        /// Update to a specific version
        #[arg(long)]
        version: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    // Load configuration
    let mut config = load_config().unwrap_or_default();

    match cli.command {
        Commands::Download {
            msvc_version,
            sdk_version,
            target,
            arch,
            no_msvc,
            no_sdk,
            no_verify,
            parallel_downloads,
        } => {
            let target_dir = target.unwrap_or_else(|| config.install_dir.clone());
            let arch: Architecture = arch.parse().map_err(|e: String| anyhow::anyhow!(e))?;

            let options = DownloadOptions {
                msvc_version,
                sdk_version,
                target_dir: target_dir.clone(),
                arch,
                host_arch: Some(Architecture::host()),
                verify_hashes: !no_verify,
                parallel_downloads: parallel_downloads.unwrap_or(config.parallel_downloads),
                http_client: None,
                progress_handler: None,
                dry_run: false,
            };

            println!("📦 msvc-kit - Downloading MSVC Build Tools\n");
            println!("Target directory: {}", target_dir.display());
            println!("Architecture: {}", arch);
            println!();

            if !no_msvc {
                println!("⬇️  Downloading MSVC compiler...");
                let msvc_info = download_msvc(&options).await?;
                println!("📁 Installing MSVC...");
                install_msvc(&msvc_info).await?;
                println!(
                    "✅ MSVC {} installed to {}",
                    msvc_info.version,
                    msvc_info.install_path.display()
                );
            }

            if !no_sdk {
                println!("\n⬇️  Downloading Windows SDK...");
                let sdk_info = download_sdk(&options).await?;
                println!("📁 Installing Windows SDK...");
                install_sdk(&sdk_info).await?;
                println!(
                    "✅ Windows SDK {} installed to {}",
                    sdk_info.version,
                    sdk_info.install_path.display()
                );
            }

            println!("\n🎉 Download complete!");
            println!("\nRun 'msvc-kit setup' to configure environment variables.");
        }

        Commands::Setup {
            dir,
            arch,
            script,
            shell,
            portable_root,
            persistent,
        } => {
            let install_dir = dir.unwrap_or_else(|| config.install_dir.clone());
            let arch: Architecture = arch.parse().map_err(|e: String| anyhow::anyhow!(e))?;

            // Find installed versions
            let msvc_versions = list_installed_msvc(&install_dir);
            let sdk_versions = list_installed_sdk(&install_dir);

            if msvc_versions.is_empty() {
                anyhow::bail!("No MSVC installation found. Run 'msvc-kit download' first.");
            }

            let msvc_version = &msvc_versions[0];
            let sdk_version = sdk_versions.first();

            // Create mock install info for environment setup
            let msvc_info = msvc_kit::installer::InstallInfo {
                component_type: "msvc".to_string(),
                version: msvc_version.version.clone(),
                install_path: msvc_version.install_path.clone().unwrap(),
                downloaded_files: vec![],
                arch,
            };

            let sdk_info = sdk_version.map(|v| msvc_kit::installer::InstallInfo {
                component_type: "sdk".to_string(),
                version: v.version.clone(),
                install_path: v.install_path.clone().unwrap(),
                downloaded_files: vec![],
                arch,
            });

            let env = setup_environment(&msvc_info, sdk_info.as_ref())?;

            if script {
                let shell_type = match shell.to_lowercase().as_str() {
                    "cmd" | "bat" => ShellType::Cmd,
                    "powershell" | "ps1" | "pwsh" => ShellType::PowerShell,
                    "bash" | "sh" => ShellType::Bash,
                    _ => ShellType::detect(),
                };

                let mut vars = get_env_vars(&env);
                if let Some(portable_root) = portable_root {
                    let install_root = install_dir.display().to_string();
                    for value in vars.values_mut() {
                        *value = value.replace(&install_root, &portable_root);
                    }
                }

                let script_content = generate_activation_script_with_vars(&vars, shell_type);
                println!("{}", script_content);
            } else if persistent {
                #[cfg(windows)]
                {
                    msvc_kit::env::write_to_registry(&env)?;
                    println!("✅ Environment variables written to registry.");
                    println!("Please restart your terminal for changes to take effect.");
                }
                #[cfg(not(windows))]
                {
                    anyhow::bail!("Persistent environment setup is only supported on Windows.");
                }
            } else {
                // Print instructions for temporary setup
                let shell_type = ShellType::detect();
                let _script = generate_activation_script(&env, shell_type);

                println!("📋 MSVC Environment Setup\n");
                println!("To activate the MSVC environment, run:\n");

                match shell_type {
                    ShellType::Cmd => {
                        println!("  msvc-kit setup --script --shell cmd > activate.bat");
                        println!("  activate.bat");
                    }
                    ShellType::PowerShell => {
                        println!(
                            "  msvc-kit setup --script --shell powershell | Invoke-Expression"
                        );
                        println!("\nOr save to a file:");
                        println!("  msvc-kit setup --script --shell powershell > activate.ps1");
                        println!("  . .\\activate.ps1");
                    }
                    ShellType::Bash => {
                        println!("  eval \"$(msvc-kit setup --script --shell bash)\"");
                    }
                }

                println!("\nFor persistent setup (Windows only):");
                println!("  msvc-kit setup --persistent");
            }
        }

        Commands::List { dir, available } => {
            let install_dir = dir.unwrap_or_else(|| config.install_dir.clone());

            if available {
                println!("📋 Fetching available versions from Microsoft...\n");

                let manifest = msvc_kit::downloader::VsManifest::fetch().await?;

                if let Some(msvc) = manifest.get_latest_msvc_version() {
                    println!("Latest MSVC version: {}", msvc);
                }
                if let Some(sdk) = manifest.get_latest_sdk_version() {
                    println!("Latest Windows SDK version: {}", sdk);
                }
            } else {
                println!("📋 Installed versions in {}\n", install_dir.display());

                let msvc_versions = list_installed_msvc(&install_dir);
                let sdk_versions = list_installed_sdk(&install_dir);

                if msvc_versions.is_empty() && sdk_versions.is_empty() {
                    println!("No installations found.");
                    println!("\nRun 'msvc-kit download' to install MSVC and Windows SDK.");
                } else {
                    if !msvc_versions.is_empty() {
                        println!("MSVC Compiler:");
                        for v in &msvc_versions {
                            println!("  - {}", v);
                        }
                    }

                    if !sdk_versions.is_empty() {
                        println!("\nWindows SDK:");
                        for v in &sdk_versions {
                            println!("  - {}", v);
                        }
                    }
                }
            }
        }

        Commands::Clean {
            dir,
            msvc_version,
            sdk_version,
            all,
            cache,
        } => {
            let install_dir = dir.unwrap_or_else(|| config.install_dir.clone());

            if all {
                println!("🗑️  Removing all installed versions...");

                if install_dir.exists() {
                    tokio::fs::remove_dir_all(&install_dir).await?;
                    println!("✅ Removed {}", install_dir.display());
                }
            } else {
                if let Some(version) = msvc_version {
                    let msvc_path = install_dir
                        .join("VC")
                        .join("Tools")
                        .join("MSVC")
                        .join(&version);
                    if msvc_path.exists() {
                        tokio::fs::remove_dir_all(&msvc_path).await?;
                        println!("✅ Removed MSVC {}", version);
                    } else {
                        println!("⚠️  MSVC {} not found", version);
                    }
                }

                if let Some(version) = sdk_version {
                    let sdk_path = install_dir
                        .join("Windows Kits")
                        .join("10")
                        .join("Include")
                        .join(&version);
                    if sdk_path.exists() {
                        // Remove SDK version from all subdirectories
                        for subdir in ["Include", "Lib", "bin"] {
                            let path = install_dir
                                .join("Windows Kits")
                                .join("10")
                                .join(subdir)
                                .join(&version);
                            if path.exists() {
                                tokio::fs::remove_dir_all(&path).await?;
                            }
                        }
                        println!("✅ Removed Windows SDK {}", version);
                    } else {
                        println!("⚠️  Windows SDK {} not found", version);
                    }
                }
            }

            if cache {
                let cache_dir = install_dir.join("downloads");
                if cache_dir.exists() {
                    tokio::fs::remove_dir_all(&cache_dir).await?;
                    println!("✅ Removed download cache");
                }
            }
        }

        Commands::Config {
            set_dir,
            set_msvc,
            set_sdk,
            reset,
        } => {
            if reset {
                config = MsvcKitConfig::default();
                save_config(&config)?;
                println!("✅ Configuration reset to defaults");
            } else if set_dir.is_some() || set_msvc.is_some() || set_sdk.is_some() {
                if let Some(dir) = set_dir {
                    config.install_dir = dir;
                }
                if let Some(msvc) = set_msvc {
                    config.default_msvc_version = Some(msvc);
                }
                if let Some(sdk) = set_sdk {
                    config.default_sdk_version = Some(sdk);
                }
                save_config(&config)?;
                println!("✅ Configuration updated");
            }

            println!("📋 Current configuration:\n");
            println!("  Install directory: {}", config.install_dir.display());
            println!(
                "  Default MSVC version: {}",
                config.default_msvc_version.as_deref().unwrap_or("latest")
            );
            println!(
                "  Default SDK version: {}",
                config.default_sdk_version.as_deref().unwrap_or("latest")
            );
            println!("  Default architecture: {}", config.default_arch);
            println!("  Verify hashes: {}", config.verify_hashes);
            println!("  Parallel downloads: {}", config.parallel_downloads);
        }

        Commands::Bundle {
            output,
            arch,
            msvc_version,
            sdk_version,
            accept_license,
            zip,
        } => {
            if !accept_license {
                println!("⚠️  License Agreement Required\n");
                println!(
                    "The MSVC compiler and Windows SDK are subject to Microsoft's license terms:"
                );
                println!("  https://visualstudio.microsoft.com/license-terms/\n");
                println!("By using --accept-license, you confirm that you have read and accepted");
                println!("Microsoft's Visual Studio License Terms.\n");
                println!("Usage:");
                println!("  msvc-kit bundle --accept-license [--output <dir>] [--arch <arch>]\n");
                anyhow::bail!(
                    "You must accept the license terms with --accept-license to proceed."
                );
            }

            let arch: Architecture = arch.parse().map_err(|e: String| anyhow::anyhow!(e))?;

            println!("📦 msvc-kit - Creating Portable MSVC Bundle\n");
            println!("Output directory: {}", output.display());
            println!("Architecture: {}", arch);
            println!();

            // Create output directory
            tokio::fs::create_dir_all(&output).await?;

            // Download options - download directly to bundle root (not runtime/)
            let options = DownloadOptions {
                msvc_version: msvc_version.clone(),
                sdk_version: sdk_version.clone(),
                target_dir: output.clone(),
                arch,
                host_arch: Some(Architecture::host()),
                verify_hashes: true,
                parallel_downloads: config.parallel_downloads,
                http_client: None,
                progress_handler: None,
                dry_run: false,
            };

            // Download MSVC
            println!("⬇️  Downloading MSVC compiler...");
            let msvc_info = download_msvc(&options).await?;
            println!("📁 Installing MSVC...");
            install_msvc(&msvc_info).await?;
            let msvc_ver = msvc_info.version.clone();
            println!("✅ MSVC {} installed", msvc_ver);

            // Download SDK
            println!("\n⬇️  Downloading Windows SDK...");
            let sdk_info = download_sdk(&options).await?;
            println!("📁 Installing Windows SDK...");
            install_sdk(&sdk_info).await?;
            let sdk_ver = sdk_info.version.clone();
            println!("✅ Windows SDK {} installed", sdk_ver);

            // Create bundle layout
            let layout = BundleLayout::from_root_with_versions(
                &output,
                &msvc_ver,
                &sdk_ver,
                arch,
                Architecture::host(),
            )?;

            // Generate and save activation scripts
            let scripts = generate_bundle_scripts(&layout)?;
            save_bundle_scripts(&layout, &scripts).await?;

            // Generate README
            let readme_content = msvc_kit::bundle::scripts::generate_bundle_readme(&layout);
            tokio::fs::write(output.join("README.txt"), &readme_content).await?;

            // Copy msvc-kit executable
            let exe_name = if cfg!(windows) {
                "msvc-kit.exe"
            } else {
                "msvc-kit"
            };
            let current_exe = std::env::current_exe()?;
            let target_exe = output.join(exe_name);
            tokio::fs::copy(&current_exe, &target_exe).await?;

            println!("\n✅ Bundle created successfully!");
            println!("\nContents:");
            println!("  {}/", output.display());
            println!("  ├── {}", exe_name);
            println!("  ├── setup.bat");
            println!("  ├── setup.ps1");
            println!("  ├── setup.sh");
            println!("  ├── README.txt");
            println!("  ├── VC/Tools/MSVC/{}/", msvc_ver);
            println!("  └── Windows Kits/10/");

            if zip {
                println!("\n📦 Creating zip archive...");
                let zip_name = format!(
                    "msvc-kit-bundle-{}-{}-{}.zip",
                    msvc_ver.replace('.', "_"),
                    sdk_ver.replace('.', "_"),
                    arch
                );
                let zip_path = output.parent().unwrap_or(&output).join(&zip_name);

                #[cfg(windows)]
                {
                    let output_str = output.display().to_string();
                    let zip_str = zip_path.display().to_string();
                    let status = std::process::Command::new("powershell")
                        .args([
                            "-NoProfile",
                            "-Command",
                            &format!(
                                "Compress-Archive -Path '{}\\*' -DestinationPath '{}' -Force",
                                output_str, zip_str
                            ),
                        ])
                        .status()?;
                    if status.success() {
                        println!("✅ Created: {}", zip_path.display());
                    } else {
                        println!("⚠️  Failed to create zip archive");
                    }
                }
                #[cfg(not(windows))]
                {
                    println!("⚠️  Zip creation is only supported on Windows");
                }
            }

            println!("\n🎉 Done! Run setup.bat (cmd) or .\\setup.ps1 (PowerShell) to activate.");
        }

        Commands::Env { dir, format } => {
            let install_dir = dir.unwrap_or_else(|| config.install_dir.clone());

            let msvc_versions = list_installed_msvc(&install_dir);
            if msvc_versions.is_empty() {
                anyhow::bail!("No MSVC installation found. Run 'msvc-kit download' first.");
            }

            let msvc_version = &msvc_versions[0];
            let sdk_versions = list_installed_sdk(&install_dir);
            let sdk_version = sdk_versions.first();

            let msvc_info = msvc_kit::installer::InstallInfo {
                component_type: "msvc".to_string(),
                version: msvc_version.version.clone(),
                install_path: msvc_version.install_path.clone().unwrap(),
                downloaded_files: vec![],
                arch: config.default_arch,
            };

            let sdk_info = sdk_version.map(|v| msvc_kit::installer::InstallInfo {
                component_type: "sdk".to_string(),
                version: v.version.clone(),
                install_path: v.install_path.clone().unwrap(),
                downloaded_files: vec![],
                arch: config.default_arch,
            });

            let env = setup_environment(&msvc_info, sdk_info.as_ref())?;
            let vars = get_env_vars(&env);

            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&vars)?);
                }
                _ => {
                    for (key, value) in &vars {
                        println!("{}={}", key, value);
                    }
                }
            }
        }

        #[cfg(feature = "self-update")]
        Commands::Update { check, version } => {
            let current_version = env!("CARGO_PKG_VERSION");

            // Determine target based on architecture
            let target = if cfg!(target_arch = "x86_64") {
                "x86_64-windows"
            } else if cfg!(target_arch = "x86") {
                "i686-windows"
            } else if cfg!(target_arch = "aarch64") {
                "aarch64-windows"
            } else {
                anyhow::bail!("Unsupported architecture for self-update");
            };

            let status = self_update::backends::github::Update::configure()
                .repo_owner("loonghao")
                .repo_name("msvc-kit")
                .bin_name("msvc-kit")
                .target(target)
                .current_version(current_version)
                .build()?;

            if check {
                println!("🔍 Checking for updates...\n");
                println!("Current version: v{}", current_version);

                match status.get_latest_release() {
                    Ok(release) => {
                        let latest = &release.version;
                        if latest != current_version {
                            println!("Latest version:  v{}", latest);
                            println!("\n📦 A new version is available!");
                            println!("Run 'msvc-kit update' to upgrade.");
                        } else {
                            println!("\n✅ You are running the latest version.");
                        }
                    }
                    Err(e) => {
                        println!("⚠️  Failed to check for updates: {}", e);
                    }
                }
            } else {
                println!("🔄 Updating msvc-kit...\n");
                println!("Current version: v{}", current_version);

                let update_result = if let Some(target_version) = version {
                    // Update to specific version
                    self_update::backends::github::Update::configure()
                        .repo_owner("loonghao")
                        .repo_name("msvc-kit")
                        .bin_name("msvc-kit")
                        .target(target)
                        .current_version(current_version)
                        .target_version_tag(&format!("v{}", target_version))
                        .build()?
                        .update()
                } else {
                    // Update to latest
                    status.update()
                };

                match update_result {
                    Ok(update_status) => {
                        if update_status.updated() {
                            println!("\n✅ Updated to v{}!", update_status.version());
                            println!("Please restart msvc-kit to use the new version.");
                        } else {
                            println!(
                                "\n✅ Already running the latest version (v{}).",
                                current_version
                            );
                        }
                    }
                    Err(e) => {
                        anyhow::bail!("Failed to update: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}
