//! Environment setup and activation script generation

use std::path::PathBuf;

use crate::error::{MsvcKitError, Result};
use crate::installer::InstallInfo;
use crate::scripts::{
    generate_absolute_scripts, generate_script, GeneratedScripts, ScriptContext, ShellType,
};
use crate::version::Architecture;

use super::{get_env_vars, MsvcEnvironment};

/// Setup MSVC environment from installation info
///
/// Creates an `MsvcEnvironment` configuration from the provided
/// installation information.
pub fn setup_environment(
    msvc_info: &InstallInfo,
    sdk_info: Option<&InstallInfo>,
) -> Result<MsvcEnvironment> {
    let host_arch = Architecture::host();
    MsvcEnvironment::from_install_info(msvc_info, sdk_info, host_arch)
}

/// Apply environment variables to the current process
///
/// This sets the environment variables in the current process,
/// allowing subsequent commands to use the MSVC toolchain.
pub fn apply_environment(env: &MsvcEnvironment) -> Result<()> {
    let vars = get_env_vars(env);

    for (key, value) in vars {
        if key == "PATH" {
            // Prepend to existing PATH
            let current_path = std::env::var("PATH").unwrap_or_default();
            let new_path = format!("{};{}", value, current_path);
            std::env::set_var("PATH", new_path);
        } else {
            std::env::set_var(&key, &value);
        }
    }

    Ok(())
}

/// Create a ScriptContext from MsvcEnvironment
fn create_script_context(env: &MsvcEnvironment) -> ScriptContext {
    // Get the root directory (parent of VC directory)
    let root = env
        .vc_install_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| env.vc_install_dir.clone());

    ScriptContext::absolute(
        root,
        &env.vc_tools_version,
        &env.windows_sdk_version,
        env.arch,
        env.host_arch,
    )
}

/// Generate an activation script for the shell
///
/// Creates a script that can be sourced/executed to set up the
/// MSVC environment in a new shell session.
pub fn generate_activation_script(env: &MsvcEnvironment, shell: ShellType) -> Result<String> {
    let ctx = create_script_context(env);
    generate_script(&ctx, shell)
}

/// Generate all activation scripts
pub fn generate_all_activation_scripts(env: &MsvcEnvironment) -> Result<GeneratedScripts> {
    let ctx = create_script_context(env);
    generate_absolute_scripts(&ctx)
}

/// Save activation script to a file
pub async fn save_activation_script(
    env: &MsvcEnvironment,
    shell: ShellType,
    output_dir: &PathBuf,
) -> Result<PathBuf> {
    let script = generate_activation_script(env, shell)?;
    let filename = format!("activate.{}", shell.script_extension());
    let path = output_dir.join(&filename);

    tokio::fs::create_dir_all(output_dir).await?;
    tokio::fs::write(&path, script).await?;

    Ok(path)
}

/// Write environment variables to Windows registry (user level)
#[cfg(windows)]
pub fn write_to_registry(env: &MsvcEnvironment) -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env_key, _) = hkcu
        .create_subkey("Environment")
        .map_err(|e| MsvcKitError::EnvSetup(format!("Failed to open registry: {}", e)))?;

    let vars = get_env_vars(env);

    for (key, value) in vars {
        if key == "PATH" {
            // Append to existing PATH
            let current: String = env_key.get_value("Path").unwrap_or_default();
            let new_path = if current.is_empty() {
                value
            } else {
                format!("{};{}", value, current)
            };
            env_key
                .set_value("Path", &new_path)
                .map_err(|e| MsvcKitError::EnvSetup(format!("Failed to set PATH: {}", e)))?;
        } else {
            env_key
                .set_value(&key, &value)
                .map_err(|e| MsvcKitError::EnvSetup(format!("Failed to set {}: {}", key, e)))?;
        }
    }

    // Broadcast environment change
    broadcast_environment_change();

    Ok(())
}

#[cfg(windows)]
fn broadcast_environment_change() {
    // This would require winapi crate for proper implementation
    // For now, just log that a restart may be needed
    tracing::info!("Environment variables updated. You may need to restart your terminal.");
}

#[cfg(not(windows))]
pub fn write_to_registry(_env: &MsvcEnvironment) -> Result<()> {
    Err(MsvcKitError::UnsupportedPlatform(
        "Registry operations are only supported on Windows".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Architecture;
    use std::path::PathBuf;

    #[test]
    fn test_shell_type_detect() {
        // Just ensure it doesn't panic
        let _ = ShellType::detect();
    }

    #[test]
    fn test_script_extension() {
        assert_eq!(ShellType::Cmd.script_extension(), "bat");
        assert_eq!(ShellType::PowerShell.script_extension(), "ps1");
        assert_eq!(ShellType::Bash.script_extension(), "sh");
    }

    fn sample_env() -> MsvcEnvironment {
        MsvcEnvironment {
            vc_install_dir: PathBuf::from("C:/toolchain/VC"),
            vc_tools_install_dir: PathBuf::from("C:/toolchain/VC/Tools/MSVC/14.40.0"),
            vc_tools_version: "14.40.0".to_string(),
            windows_sdk_dir: PathBuf::from("C:/toolchain/Windows Kits/10"),
            windows_sdk_version: "10.0.22621.0".to_string(),
            include_paths: vec![PathBuf::from("C:/toolchain/include")],
            lib_paths: vec![PathBuf::from("C:/toolchain/lib")],
            bin_paths: vec![
                PathBuf::from("C:/toolchain/bin1"),
                PathBuf::from("C:/toolchain/bin2"),
            ],
            arch: Architecture::X64,
            host_arch: Architecture::X64,
        }
    }

    #[test]
    fn test_generate_activation_script() {
        let env = sample_env();
        let script = generate_activation_script(&env, ShellType::Cmd).unwrap();

        assert!(script.contains("INCLUDE"));
        assert!(script.contains("LIB"));
        assert!(script.contains("PATH"));
    }

    #[test]
    fn test_generate_activation_script_powershell() {
        let env = sample_env();
        let script = generate_activation_script(&env, ShellType::PowerShell).unwrap();

        assert!(script.contains("$env:"));
        assert!(script.contains("14.40.0"));
    }

    #[test]
    fn test_generate_activation_script_bash() {
        let env = sample_env();
        let script = generate_activation_script(&env, ShellType::Bash).unwrap();

        assert!(script.contains("export"));
        assert!(script.contains("14.40.0"));
    }

    #[test]
    fn test_generate_all_activation_scripts() {
        let env = sample_env();
        let scripts = generate_all_activation_scripts(&env).unwrap();

        assert!(!scripts.cmd.is_empty());
        assert!(!scripts.powershell.is_empty());
        assert!(!scripts.bash.is_empty());
        // Absolute scripts don't have readme
        assert!(scripts.readme.is_none());
    }

    #[test]
    fn test_apply_environment() {
        let env = sample_env();

        // Save original PATH
        let original_path = std::env::var("PATH").ok();

        // Apply environment
        apply_environment(&env).unwrap();

        // Verify some env vars are set
        assert!(std::env::var("VCToolsVersion").is_ok());
        assert!(std::env::var("WindowsSDKVersion").is_ok());

        // Restore original PATH if it existed
        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        }

        // Clean up test env vars
        std::env::remove_var("VCToolsVersion");
        std::env::remove_var("WindowsSDKVersion");
    }

    #[test]
    fn test_create_script_context() {
        let env = sample_env();
        let ctx = create_script_context(&env);

        assert!(!ctx.portable);
        assert!(ctx.root.is_some());
        assert_eq!(ctx.msvc_version, "14.40.0");
        assert_eq!(ctx.sdk_version, "10.0.22621.0");
        assert_eq!(ctx.arch, Architecture::X64);
        assert_eq!(ctx.host_arch, Architecture::X64);
    }

    #[tokio::test]
    async fn test_save_activation_script() {
        let temp_dir = tempfile::tempdir().unwrap();
        let env = sample_env();

        let path = save_activation_script(&env, ShellType::Cmd, &temp_dir.path().to_path_buf())
            .await
            .unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with("activate.bat"));

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("14.40.0"));
    }

    #[tokio::test]
    async fn test_save_activation_script_powershell() {
        let temp_dir = tempfile::tempdir().unwrap();
        let env = sample_env();

        let path =
            save_activation_script(&env, ShellType::PowerShell, &temp_dir.path().to_path_buf())
                .await
                .unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with("activate.ps1"));
    }

    #[tokio::test]
    async fn test_save_activation_script_bash() {
        let temp_dir = tempfile::tempdir().unwrap();
        let env = sample_env();

        let path = save_activation_script(&env, ShellType::Bash, &temp_dir.path().to_path_buf())
            .await
            .unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with("activate.sh"));
    }

    #[cfg(not(windows))]
    #[test]
    fn test_write_to_registry_unsupported() {
        let env = sample_env();
        let result = write_to_registry(&env);
        assert!(result.is_err());
    }
}
