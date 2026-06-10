//! msvc-kit CLI - Portable MSVC Build Tools installer and manager

use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use clap::{CommandFactory, Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use msvc_kit::bundle::{generate_bundle_scripts, save_bundle_scripts, BundleLayout};
use msvc_kit::env::generate_activation_script;
use msvc_kit::query::{QueryComponent, QueryOptions, QueryProperty};
use msvc_kit::version::{list_installed_msvc, list_installed_sdk, Architecture};
use msvc_kit::{
    download_msvc, download_sdk, generate_script, get_env_vars, load_config, query_installation,
    save_config, setup_environment, DownloadOptions, MsvcComponent, MsvcKitConfig, ScriptContext,
    ShellType,
};

fn infer_self_update_install_root(exe_path: &std::path::Path) -> Option<PathBuf> {
    let exe_dir = exe_path.parent()?;
    if exe_dir.file_name().is_some_and(|name| name == "bin") {
        return exe_dir.parent().map(std::path::Path::to_path_buf);
    }

    Some(exe_dir.to_path_buf())
}

fn updated_executable_candidates(
    original_exe: &std::path::Path,
    install_root: &std::path::Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(name) = original_exe.file_name() {
        for candidate in [
            original_exe.to_path_buf(),
            install_root.join("bin").join(name),
            install_root.join(name),
        ] {
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

fn parse_msvc_kit_version(output: &str) -> Option<&str> {
    let mut parts = output.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some("msvc-kit"), Some(version), None) => Some(version),
        _ => None,
    }
}

fn verify_updated_executable(
    original_exe: &std::path::Path,
    install_root: &std::path::Path,
    expected_version: &str,
) -> Result<PathBuf, String> {
    let mut errors = Vec::new();
    for candidate in updated_executable_candidates(original_exe, install_root) {
        if !candidate.exists() {
            errors.push(format!("{} does not exist", candidate.display()));
            continue;
        }

        let output = ProcessCommand::new(&candidate)
            .arg("--version")
            .output()
            .map_err(|e| format!("failed to run {} --version: {}", candidate.display(), e))?;

        if !output.status.success() {
            errors.push(format!(
                "{} --version exited with {:?}",
                candidate.display(),
                output.status.code()
            ));
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let actual = parse_msvc_kit_version(stdout.trim()).ok_or_else(|| {
            format!(
                "{} --version returned unexpected output: {}",
                candidate.display(),
                stdout.trim()
            )
        })?;

        if actual == expected_version {
            return Ok(candidate);
        }

        errors.push(format!(
            "{} reports version {}, expected {}",
            candidate.display(),
            actual,
            expected_version
        ));
    }

    Err(format!(
        "updated executable verification failed: {}",
        errors.join("; ")
    ))
}

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
    command: Option<Commands>,
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
        #[arg(short = 't', long, visible_short_alias = 'd', visible_alias = "dir")]
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

        /// Include optional MSVC components (spectre, mfc, atl, asan, uwp, custom:<pattern>)
        /// Can be specified multiple times
        #[arg(long = "include-component", value_name = "COMPONENT")]
        include_components: Vec<String>,

        /// Exclude packages matching pattern (case-insensitive substring match)
        /// Can be specified multiple times
        #[arg(long = "exclude-pattern", value_name = "PATTERN")]
        exclude_patterns: Vec<String>,
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

    /// Query installed components for paths, environment variables, and tool locations
    Query {
        /// Installation directory
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// Target architecture (x64, x86, arm64)
        #[arg(short, long, default_value = "x64")]
        arch: String,

        /// Component to query (all, msvc, sdk)
        #[arg(long, default_value = "all")]
        component: String,

        /// Property to retrieve (all, path, env, tools, version, include, lib)
        #[arg(short, long, default_value = "all")]
        property: String,

        /// Specific MSVC version to query (default: latest installed)
        #[arg(long)]
        msvc_version: Option<String>,

        /// Specific SDK version to query (default: latest installed)
        #[arg(long)]
        sdk_version: Option<String>,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Install a downloaded MSVC toolchain into Visual Studio (UBT discovery)
    InstallIntoVs {
        /// Downloaded MSVC toolchain directory (e.g. C:\msvc-kit\14.36)
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// Only check installed VS instances and registered MSVC versions
        #[arg(long)]
        check: bool,

        /// Auto-detect latest downloaded toolchain in the config install dir
        #[arg(long)]
        auto: bool,
    },

    /// Create a portable bundle with MSVC toolchain (downloads components locally)
    Bundle {
        /// Output directory for the bundle
        #[arg(short, long, default_value = "./msvc-bundle")]
        output: PathBuf,

        /// Target architecture (x64, x86, arm64)
        #[arg(short, long, default_value = "x64")]
        arch: String,

        /// Host architecture for cross-compilation (x64, x86, arm64)
        /// Defaults to current system architecture
        #[arg(long)]
        host_arch: Option<String>,

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

    // Handle the case where no subcommand is provided (for winget compatibility)
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            // Print help and exit with code 0 for winget validation
            Cli::command().print_help().unwrap();
            return Ok(());
        }
    };

    match command {
        Commands::Download {
            msvc_version,
            sdk_version,
            target,
            arch,
            no_msvc,
            no_sdk,
            no_verify,
            parallel_downloads,
            include_components,
            exclude_patterns,
        } => {
            let target_dir = target.unwrap_or_else(|| config.install_dir.clone());
            let arch: Architecture = arch.parse().map_err(|e: String| anyhow::anyhow!(e))?;

            // Parse component strings into MsvcComponent enum values
            let components = include_components
                .iter()
                .filter_map(|s| {
                    s.parse::<MsvcComponent>()
                        .map_err(|e| eprintln!("Warning: {}", e))
                        .ok()
                })
                .collect();

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
                cache_manager: None,
                dry_run: false,
                include_components: components,
                exclude_patterns,
            };

            println!("msvc-kit - Downloading MSVC Build Tools\n");
            println!("Target directory: {}", target_dir.display());
            println!("Architecture: {}", arch);
            println!();

            if !no_msvc {
                println!("Downloading MSVC compiler...");
                let mut msvc_info = download_msvc(&options).await?;
                println!("Extracting MSVC packages...");
                msvc_kit::extract_and_finalize_msvc(&mut msvc_info).await?;
                println!(
                    "MSVC {} installed to {}",
                    msvc_info.version,
                    target_dir.display()
                );
            }

            if !no_sdk {
                println!("\nDownloading Windows SDK...");
                let sdk_info = download_sdk(&options).await?;
                println!("Extracting SDK packages...");
                msvc_kit::extract_and_finalize_sdk(&sdk_info).await?;
                println!(
                    "Windows SDK {} installed to {}",
                    sdk_info.version,
                    target_dir.display()
                );
            }

            println!("\nDownload complete!");
            println!("\nRun 'msvc-kit setup' to configure environment variables.");
            println!(
                "Run 'msvc-kit query --dir {}' to inspect installed paths.",
                target_dir.display()
            );
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

                // Create script context based on whether portable root is specified
                let ctx = if let Some(ref _portable_root) = portable_root {
                    // Use portable mode with relative paths
                    ScriptContext::portable(
                        &env.vc_tools_version,
                        &env.windows_sdk_version,
                        arch,
                        arch,
                    )
                } else {
                    // Use absolute mode with actual paths
                    ScriptContext::absolute(
                        install_dir.clone(),
                        &env.vc_tools_version,
                        &env.windows_sdk_version,
                        arch,
                        arch,
                    )
                };

                let script_content = generate_script(&ctx, shell_type)?;
                println!("{}", script_content);
            } else if persistent {
                #[cfg(windows)]
                {
                    msvc_kit::env::write_to_registry(&env)?;
                    println!("Environment variables written to registry.");
                    println!("Please restart your terminal for changes to take effect.");
                }
                #[cfg(not(windows))]
                {
                    anyhow::bail!("Persistent environment setup is only supported on Windows.");
                }
            } else {
                // Print instructions for temporary setup
                let shell_type = ShellType::detect();
                let _script = generate_activation_script(&env, shell_type)?;

                println!("MSVC Environment Setup\n");
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
                println!("Fetching available versions from Microsoft...\n");

                let manifest = msvc_kit::downloader::VsManifest::fetch().await?;

                if let Some(msvc) = manifest.get_latest_msvc_version() {
                    println!("Latest MSVC version: {}", msvc);
                }
                if let Some(sdk) = manifest.get_latest_sdk_version() {
                    println!("Latest Windows SDK version: {}", sdk);
                }
            } else {
                println!("Installed versions in {}\n", install_dir.display());

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
                println!("Removing all installed versions...");

                if install_dir.exists() {
                    tokio::fs::remove_dir_all(&install_dir).await?;
                    println!("Removed {}", install_dir.display());
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
                        println!("Removed MSVC {}", version);
                    } else {
                        println!("MSVC {} not found", version);
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
                        println!("Removed Windows SDK {}", version);
                    } else {
                        println!("Windows SDK {} not found", version);
                    }
                }
            }

            if cache {
                let cache_dir = install_dir.join("downloads");
                if cache_dir.exists() {
                    tokio::fs::remove_dir_all(&cache_dir).await?;
                    println!("Removed download cache");
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
                println!("Configuration reset to defaults");
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
                println!("Configuration updated");
            }

            println!("Current configuration:\n");
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

        Commands::InstallIntoVs { dir, check, auto } => {
            if check {
                // --check mode: enumerate VS instances and their registered MSVC versions
                println!("Checking Visual Studio installations...\n");
                let instances = msvc_kit::install_into_vs::find_vs_instances();
                if instances.is_empty() {
                    println!("No Visual Studio instances with VC Tools found.");
                    println!("\nTroubleshooting:");
                    println!("  - Ensure Visual Studio BuildTools 2022 is installed");
                    println!("  - Ensure the \"VC++ tools\" workload is selected");
                    println!("  - Run from an elevated (admin) prompt if needed");
                } else {
                    for inst in &instances {
                        println!("{} ({})", inst.label, inst.version);
                        println!("   Path: {}", inst.install_path.display());
                        let versions =
                            msvc_kit::install_into_vs::list_vs_msvc_versions(&inst.install_path);
                        if versions.is_empty() {
                            println!("   Registered MSVC: (none)");
                        } else {
                            println!("   Registered MSVC:");
                            for v in &versions {
                                println!("    - {}", v);
                            }
                        }
                        let writable =
                            msvc_kit::install_into_vs::can_write_to_vs(&inst.install_path);
                        println!(
                            "   Writable: {}",
                            if writable {
                                "yes"
                            } else {
                                "no (requires admin)"
                            }
                        );
                        println!();
                    }
                }
            } else if auto {
                // --auto mode: find the latest download in the config install dir
                let install_dir = config.install_dir.clone();
                println!(
                    "Auto-detecting downloaded MSVC toolchain in {}...\n",
                    install_dir.display()
                );
                let msvc_versions = msvc_kit::version::list_installed_msvc(&install_dir);
                if msvc_versions.is_empty() {
                    anyhow::bail!(
                        "No downloaded MSVC toolchain found. Run 'msvc-kit download' first."
                    );
                }
                for v in &msvc_versions {
                    println!(
                        "   Found: {} at {}",
                        v.version,
                        v.install_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    );
                }
                let latest = &msvc_versions[0];
                let source_dir = latest
                    .install_path
                    .as_ref()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap();
                println!(
                    "\nInstalling latest MSVC {} from {}...\n",
                    latest.version,
                    source_dir.display()
                );

                let instances = msvc_kit::install_into_vs::find_vs_instances();
                if instances.is_empty() {
                    anyhow::bail!("No Visual Studio instance found. Cannot install.");
                }

                let target = &instances[0];
                match msvc_kit::install_into_vs::install_into_vs(source_dir, target) {
                    Ok(result) => {
                        println!(
                            "MSVC {} installed into {} ({})",
                            latest.version,
                            target.label,
                            target.install_path.display()
                        );
                        println!("\nRegistered MSVC toolchains:");
                        for v in &result.registered_versions {
                            println!("  - {}", v);
                        }
                    }
                    Err(e) => {
                        anyhow::bail!("Installation failed: {}\n\nTip: Run from an elevated (admin) command prompt.", e);
                    }
                }
            } else if let Some(ref source_dir) = dir {
                // Explicit directory mode
                println!(
                    "Installing MSVC toolchain from {}...\n",
                    source_dir.display()
                );

                let instances = msvc_kit::install_into_vs::find_vs_instances();
                if instances.is_empty() {
                    anyhow::bail!("No Visual Studio instance found. Cannot install.");
                }

                for (i, inst) in instances.iter().enumerate() {
                    println!(
                        "  [{}.] {} ({})",
                        i + 1,
                        inst.label,
                        inst.install_path.display()
                    );
                }

                let target = &instances[0];
                println!(
                    "\nUsing: {} ({})",
                    target.label,
                    target.install_path.display()
                );

                match msvc_kit::install_into_vs::install_into_vs(source_dir, target) {
                    Ok(result) => {
                        println!("Installation complete!");
                        println!("\nRegistered MSVC toolchains:");
                        for v in &result.registered_versions {
                            println!("  - {}", v);
                        }
                    }
                    Err(e) => {
                        anyhow::bail!("Installation failed: {}\n\nTip: Run from an elevated (admin) command prompt.", e);
                    }
                }
            } else {
                // No flags: show usage
                println!("msvc-kit install-into-vs: Install a downloaded MSVC toolchain into Visual Studio\n");
                println!("Usage:");
                println!("  msvc-kit install-into-vs --check          # Check VS instances and registered versions");
                println!("  msvc-kit install-into-vs --dir <PATH>     # Install from a specific directory");
                println!("  msvc-kit install-into-vs --auto           # Auto-detect and install latest download\n");
                println!("Examples:");
                println!("  msvc-kit install-into-vs --dir C:\\msvc-kit\\14.36");
                println!("  msvc-kit install-into-vs --auto\n");
                println!("This command usually requires administrator privileges.");
            }
        }

        Commands::Bundle {
            output,
            arch,
            host_arch,
            msvc_version,
            sdk_version,
            accept_license,
            zip,
        } => {
            if !accept_license {
                println!("License Agreement Required\n");
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
            let host_arch: Architecture = host_arch
                .map(|s| s.parse().map_err(|e: String| anyhow::anyhow!(e)))
                .transpose()?
                .unwrap_or_else(Architecture::host);

            println!("msvc-kit - Creating Portable MSVC Bundle\n");
            println!("Output directory: {}", output.display());
            println!("Target architecture: {}", arch);
            println!("Host architecture: {}", host_arch);
            println!();

            // Create output directory
            tokio::fs::create_dir_all(&output).await?;

            // Download options - download directly to bundle root (not runtime/)
            let options = DownloadOptions {
                msvc_version: msvc_version.clone(),
                sdk_version: sdk_version.clone(),
                target_dir: output.clone(),
                arch,
                host_arch: Some(host_arch),
                verify_hashes: true,
                parallel_downloads: config.parallel_downloads,
                http_client: None,
                progress_handler: None,
                cache_manager: None,
                dry_run: false,
                include_components: Default::default(),
                exclude_patterns: Default::default(),
            };

            // Download and extract MSVC
            println!("Downloading MSVC compiler...");
            let mut msvc_info = download_msvc(&options).await?;
            println!("Extracting MSVC packages...");
            msvc_kit::extract_and_finalize_msvc(&mut msvc_info).await?;
            let msvc_ver = msvc_info.version.clone();
            println!("MSVC {} installed", msvc_ver);

            // Download and extract SDK
            println!("\nDownloading Windows SDK...");
            let sdk_info = download_sdk(&options).await?;
            println!("Extracting SDK packages...");
            msvc_kit::extract_and_finalize_sdk(&sdk_info).await?;
            let sdk_ver = sdk_info.version.clone();
            println!("Windows SDK {} installed", sdk_ver);

            // Create bundle layout
            let layout = BundleLayout::from_root_with_versions(
                &output, &msvc_ver, &sdk_ver, arch, host_arch,
            )?;

            // Generate and save activation scripts (includes README)
            let scripts = generate_bundle_scripts(&layout)?;
            save_bundle_scripts(&layout, &scripts).await?;

            // Copy msvc-kit executable
            let exe_name = if cfg!(windows) {
                "msvc-kit.exe"
            } else {
                "msvc-kit"
            };
            let current_exe = std::env::current_exe()?;
            let target_exe = output.join(exe_name);
            tokio::fs::copy(&current_exe, &target_exe).await?;

            println!("\nBundle created successfully!");
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
                println!("\nCreating zip archive...");
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
                        println!("Created: {}", zip_path.display());
                    } else {
                        println!("Failed to create zip archive");
                    }
                }
                #[cfg(not(windows))]
                {
                    println!("Zip creation is only supported on Windows");
                }
            }

            println!("\nDone! Run setup.bat (cmd) or .\\setup.ps1 (PowerShell) to activate.");
        }

        Commands::Query {
            dir,
            arch,
            component,
            property,
            msvc_version,
            sdk_version,
            format,
        } => {
            let install_dir = dir.unwrap_or_else(|| config.install_dir.clone());
            let arch: Architecture = arch.parse().map_err(|e: String| anyhow::anyhow!(e))?;
            let component: QueryComponent =
                component.parse().map_err(|e: String| anyhow::anyhow!(e))?;
            let property: QueryProperty =
                property.parse().map_err(|e: String| anyhow::anyhow!(e))?;

            let options = QueryOptions::builder()
                .install_dir(&install_dir)
                .arch(arch)
                .component(component)
                .property(property);

            let options = if let Some(ref ver) = msvc_version {
                options.msvc_version(ver)
            } else {
                options
            };

            let options = if let Some(ref ver) = sdk_version {
                options.sdk_version(ver)
            } else {
                options
            };

            let options = options.build();
            let result = query_installation(&options)?;

            match format.as_str() {
                "json" => {
                    // JSON output: filter by property
                    let json = match property {
                        QueryProperty::All => serde_json::to_string_pretty(&result.to_json())?,
                        QueryProperty::Path => {
                            let mut paths = serde_json::Map::new();
                            paths.insert(
                                "install_dir".to_string(),
                                serde_json::json!(result.install_dir),
                            );
                            if let Some(ref msvc) = result.msvc {
                                paths.insert(
                                    "msvc_path".to_string(),
                                    serde_json::json!(msvc.install_path),
                                );
                            }
                            if let Some(ref sdk) = result.sdk {
                                paths.insert(
                                    "sdk_path".to_string(),
                                    serde_json::json!(sdk.install_path),
                                );
                            }
                            serde_json::to_string_pretty(&paths)?
                        }
                        QueryProperty::Env => serde_json::to_string_pretty(&result.env_vars)?,
                        QueryProperty::Tools => serde_json::to_string_pretty(&result.tools)?,
                        QueryProperty::Version => {
                            let mut versions = serde_json::Map::new();
                            if let Some(v) = result.msvc_version() {
                                versions.insert(
                                    "msvc".to_string(),
                                    serde_json::Value::String(v.to_string()),
                                );
                            }
                            if let Some(v) = result.sdk_version() {
                                versions.insert(
                                    "sdk".to_string(),
                                    serde_json::Value::String(v.to_string()),
                                );
                            }
                            serde_json::to_string_pretty(&versions)?
                        }
                        QueryProperty::Include => {
                            let paths: Vec<&std::path::PathBuf> = result.all_include_paths();
                            serde_json::to_string_pretty(&paths)?
                        }
                        QueryProperty::Lib => {
                            let paths: Vec<&std::path::PathBuf> = result.all_lib_paths();
                            serde_json::to_string_pretty(&paths)?
                        }
                    };
                    println!("{}", json);
                }
                _ => {
                    // Human-readable text output
                    match property {
                        QueryProperty::All => {
                            print!("{}", result.format_summary());
                            if !result.env_vars.is_empty() {
                                println!("\nEnvironment Variables:");
                                let mut sorted_vars: Vec<_> = result.env_vars.iter().collect();
                                sorted_vars.sort_by_key(|(k, _)| k.as_str());
                                for (key, value) in sorted_vars {
                                    println!("  {}={}", key, value);
                                }
                            }
                        }
                        QueryProperty::Path => {
                            println!("install_dir={}", result.install_dir.display());
                            if let Some(ref msvc) = result.msvc {
                                println!("msvc_path={}", msvc.install_path.display());
                            }
                            if let Some(ref sdk) = result.sdk {
                                println!("sdk_path={}", sdk.install_path.display());
                            }
                        }
                        QueryProperty::Env => {
                            let mut sorted_vars: Vec<_> = result.env_vars.iter().collect();
                            sorted_vars.sort_by_key(|(k, _)| k.as_str());
                            for (key, value) in sorted_vars {
                                println!("{}={}", key, value);
                            }
                        }
                        QueryProperty::Tools => {
                            let mut sorted_tools: Vec<_> = result.tools.iter().collect();
                            sorted_tools.sort_by_key(|(k, _)| k.as_str());
                            for (name, path) in sorted_tools {
                                println!("{}={}", name, path.display());
                            }
                        }
                        QueryProperty::Version => {
                            if let Some(v) = result.msvc_version() {
                                println!("msvc={}", v);
                            }
                            if let Some(v) = result.sdk_version() {
                                println!("sdk={}", v);
                            }
                        }
                        QueryProperty::Include => {
                            for path in result.all_include_paths() {
                                println!("{}", path.display());
                            }
                        }
                        QueryProperty::Lib => {
                            for path in result.all_lib_paths() {
                                println!("{}", path.display());
                            }
                        }
                    }
                }
            }
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
            let current_exe = std::env::current_exe()
                .map_err(|e| anyhow::anyhow!("Failed to locate current executable: {}", e))?;
            let install_root = infer_self_update_install_root(&current_exe)
                .ok_or_else(|| anyhow::anyhow!("Failed to infer msvc-kit install directory"))?;
            let install_root_str = install_root.to_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "msvc-kit install directory is not valid UTF-8: {}",
                    install_root.display()
                )
            })?;

            // Configure axoupdater with GitHub release source (no cargo-dist receipt needed)
            let source = axoupdater::ReleaseSource {
                release_type: axoupdater::ReleaseSourceType::GitHub,
                owner: "loonghao".to_string(),
                name: "msvc-kit".to_string(),
                app_name: "msvc-kit".to_string(),
            };

            let mut updater = axoupdater::AxoUpdater::new_for("msvc-kit");
            updater.set_release_source(source);
            updater.set_install_dir(install_root_str);
            updater
                .set_current_version(
                    current_version
                        .parse()
                        .map_err(|e| anyhow::anyhow!("Failed to parse current version: {}", e))?,
                )
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            // Disable installer output noise
            updater.disable_installer_output();

            // Use GitHub token from environment if available to avoid 403 rate limiting
            if let Ok(token) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN"))
            {
                if !token.is_empty() {
                    updater.set_github_token(&token);
                }
            }

            if let Some(ref target_version) = version {
                updater.configure_version_specifier(axoupdater::UpdateRequest::SpecificVersion(
                    target_version.clone(),
                ));
                updater.always_update(true);
            }

            if check {
                println!("Checking for updates...\n");
                println!("Current version: v{}", current_version);

                match updater.query_new_version().await {
                    Ok(Some(new_version)) => {
                        println!("Latest version:  v{}", new_version);
                        println!("\nA new version is available!");
                        println!("Run 'msvc-kit update' to upgrade.");
                    }
                    Ok(None) => {
                        println!("\nYou are running the latest version.");
                    }
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        println!("Failed to check for updates: {}", err_msg);
                        if err_msg.contains("403") || err_msg.contains("rate") {
                            println!("\nHint: GitHub API rate limit may have been reached.");
                            println!("Set GITHUB_TOKEN or GH_TOKEN environment variable to authenticate.");
                        }
                    }
                }
            } else {
                println!("Updating msvc-kit...\n");
                println!("Current version: v{}", current_version);

                match updater.run().await {
                    Ok(Some(result)) => {
                        let verified_exe = verify_updated_executable(
                            &current_exe,
                            result.install_prefix.as_std_path(),
                            &result.new_version.to_string(),
                        )
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Update installer completed, but the installed executable could not be verified: {}",
                                e
                            )
                        })?;
                        println!("\nUpdated to v{}!", result.new_version);
                        println!("Verified executable: {}", verified_exe.display());
                        println!("Please restart msvc-kit to use the new version.");
                    }
                    Ok(None) => {
                        println!(
                            "\nAlready running the latest version (v{}).",
                            current_version
                        );
                    }
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        let mut hint = String::new();
                        if err_msg.contains("403") || err_msg.contains("rate") {
                            hint = "\nHint: GitHub API rate limit may have been reached. Set GITHUB_TOKEN or GH_TOKEN environment variable to authenticate."
                                .to_string();
                        }
                        anyhow::bail!("Failed to update: {}{}", e, hint);
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_self_update_install_root_strips_bin_directory() {
        let root_dir = PathBuf::from("msvc-kit");
        let exe = root_dir.join("bin").join("msvc-kit");

        let root = infer_self_update_install_root(&exe).unwrap();

        assert_eq!(root, root_dir);
    }

    #[test]
    fn infer_self_update_install_root_uses_parent_for_non_bin_directory() {
        let root_dir = PathBuf::from("tools");
        let exe = root_dir.join("msvc-kit");

        let root = infer_self_update_install_root(&exe).unwrap();

        assert_eq!(root, root_dir);
    }

    #[test]
    fn updated_executable_candidates_include_legacy_and_cargo_dist_layouts() {
        let root = PathBuf::from("tools");
        let exe = root.join("msvc-kit");

        let candidates = updated_executable_candidates(&exe, &root);

        assert_eq!(
            candidates,
            vec![root.join("msvc-kit"), root.join("bin").join("msvc-kit"),]
        );
    }

    #[test]
    fn parse_msvc_kit_version_accepts_exact_version_output() {
        assert_eq!(parse_msvc_kit_version("msvc-kit 0.2.13"), Some("0.2.13"));
    }

    #[test]
    fn parse_msvc_kit_version_rejects_unexpected_output() {
        assert_eq!(parse_msvc_kit_version("msvc-kit version 0.2.13"), None);
        assert_eq!(parse_msvc_kit_version("0.2.13"), None);
    }

    #[test]
    fn verify_updated_executable_reports_missing_candidates() {
        let temp = tempfile::tempdir().expect("temp dir");
        let exe = temp.path().join("bin").join("msvc-kit.exe");

        let err = verify_updated_executable(&exe, temp.path(), "9.9.9").unwrap_err();

        assert!(err.contains("updated executable verification failed"));
        assert!(err.contains("does not exist"));
    }
}
