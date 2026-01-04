//! Script generation for MSVC environment activation
//!
//! This module provides unified script generation for both portable bundles
//! and installed MSVC environments. It uses askama templates for maintainability.
//!
//! # Supported Shells
//!
//! - CMD (Windows Command Prompt)
//! - PowerShell
//! - Bash (Git Bash, WSL)
//!
//! # Script Types
//!
//! - **Portable scripts**: Use relative paths (`%~dp0`, `$PSScriptRoot`, `$SCRIPT_DIR`)
//!   for bundles that can be moved to any location
//! - **Absolute scripts**: Use absolute paths for installed environments

use crate::error::{MsvcKitError, Result};
use crate::version::Architecture;
use askama::Template;
use std::path::PathBuf;

/// Shell type for script generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellType {
    /// Windows Command Prompt (cmd.exe)
    Cmd,
    /// PowerShell
    PowerShell,
    /// Bash/sh (for Git Bash, WSL, etc.)
    Bash,
}

impl ShellType {
    /// Detect the current shell type
    pub fn detect() -> Self {
        // Check for PowerShell
        if std::env::var("PSModulePath").is_ok() {
            return ShellType::PowerShell;
        }

        // Check for bash
        if std::env::var("BASH").is_ok()
            || std::env::var("SHELL")
                .map(|s| s.contains("bash"))
                .unwrap_or(false)
        {
            return ShellType::Bash;
        }

        // Default to cmd on Windows
        #[cfg(windows)]
        return ShellType::Cmd;

        #[cfg(not(windows))]
        return ShellType::Bash;
    }

    /// Get the file extension for this shell's scripts
    pub fn script_extension(&self) -> &'static str {
        match self {
            ShellType::Cmd => "bat",
            ShellType::PowerShell => "ps1",
            ShellType::Bash => "sh",
        }
    }

    /// Get the script filename
    pub fn script_filename(&self, base_name: &str) -> String {
        format!("{}.{}", base_name, self.script_extension())
    }
}

impl std::fmt::Display for ShellType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellType::Cmd => write!(f, "cmd"),
            ShellType::PowerShell => write!(f, "powershell"),
            ShellType::Bash => write!(f, "bash"),
        }
    }
}

/// Context for generating MSVC environment scripts
///
/// This struct contains all the information needed to generate activation
/// scripts for any shell type. It can be created from either a `BundleLayout`
/// or an `MsvcEnvironment`.
#[derive(Debug, Clone)]
pub struct ScriptContext {
    /// MSVC version (e.g., "14.44.34823")
    pub msvc_version: String,
    /// Windows SDK version (e.g., "10.0.26100.0")
    pub sdk_version: String,
    /// Target architecture
    pub arch: Architecture,
    /// Host architecture
    pub host_arch: Architecture,
    /// Whether to use portable (relative) paths
    pub portable: bool,
    /// Root path (only used for absolute scripts)
    pub root: Option<PathBuf>,
}

impl ScriptContext {
    /// Create a portable script context (uses relative paths)
    pub fn portable(
        msvc_version: impl Into<String>,
        sdk_version: impl Into<String>,
        arch: Architecture,
        host_arch: Architecture,
    ) -> Self {
        Self {
            msvc_version: msvc_version.into(),
            sdk_version: sdk_version.into(),
            arch,
            host_arch,
            portable: true,
            root: None,
        }
    }

    /// Create an absolute script context (uses absolute paths)
    pub fn absolute(
        root: PathBuf,
        msvc_version: impl Into<String>,
        sdk_version: impl Into<String>,
        arch: Architecture,
        host_arch: Architecture,
    ) -> Self {
        Self {
            msvc_version: msvc_version.into(),
            sdk_version: sdk_version.into(),
            arch,
            host_arch,
            portable: false,
            root: Some(root),
        }
    }

    /// Get the host architecture directory name (e.g., "Hostx64")
    pub fn host_arch_dir(&self) -> &'static str {
        self.host_arch.msvc_host_dir()
    }

    /// Get the target architecture directory name (e.g., "x64")
    pub fn target_arch_dir(&self) -> &'static str {
        self.arch.msvc_target_dir()
    }

    /// Get the root path expression for the given shell
    ///
    /// For portable scripts, returns shell-specific relative path expressions.
    /// For absolute scripts, returns the actual root path.
    fn root_expr(&self, shell: ShellType) -> String {
        if self.portable {
            match shell {
                ShellType::Cmd => "%BUNDLE_ROOT%".to_string(),
                ShellType::PowerShell => "$BundleRoot".to_string(),
                ShellType::Bash => "$BUNDLE_ROOT".to_string(),
            }
        } else {
            let root = self
                .root
                .as_ref()
                .expect("root path required for absolute scripts");
            match shell {
                ShellType::Cmd | ShellType::PowerShell => root.to_string_lossy().to_string(),
                ShellType::Bash => {
                    // Convert Windows path to Unix-style for bash
                    root.to_string_lossy()
                        .replace('\\', "/")
                        .replace("C:", "/c")
                        .replace("D:", "/d")
                }
            }
        }
    }
}

// ==================== Template Structs ====================

/// CMD script template (used for both portable and absolute)
#[derive(Template)]
#[template(path = "setup.bat.txt")]
struct CmdScriptTemplate<'a> {
    msvc_version: &'a str,
    sdk_version: &'a str,
    arch: String,
    host_arch: String,
    target_arch: String,
}

/// PowerShell script template (used for both portable and absolute)
#[derive(Template)]
#[template(path = "setup.ps1.txt")]
struct PowerShellScriptTemplate<'a> {
    msvc_version: &'a str,
    sdk_version: &'a str,
    arch: String,
    host_arch: String,
    target_arch: String,
}

/// Bash script template (used for both portable and absolute)
#[derive(Template)]
#[template(path = "setup.sh.txt")]
struct BashScriptTemplate<'a> {
    msvc_version: &'a str,
    sdk_version: &'a str,
    arch: String,
    host_arch: String,
    target_arch: String,
}

/// README template
#[derive(Template)]
#[template(path = "readme.txt")]
struct ReadmeTemplate<'a> {
    msvc_version: &'a str,
    sdk_version: &'a str,
    arch: String,
}

// ==================== Generated Scripts ====================

/// Collection of generated scripts
#[derive(Debug, Clone)]
pub struct GeneratedScripts {
    /// CMD activation script content
    pub cmd: String,
    /// PowerShell activation script content
    pub powershell: String,
    /// Bash activation script content
    pub bash: String,
    /// README content (only for portable bundles)
    pub readme: Option<String>,
}

impl GeneratedScripts {
    /// Get script content by shell type
    pub fn get(&self, shell: ShellType) -> &str {
        match shell {
            ShellType::Cmd => &self.cmd,
            ShellType::PowerShell => &self.powershell,
            ShellType::Bash => &self.bash,
        }
    }
}

// ==================== Public API ====================

/// Generate portable activation scripts for a bundle
///
/// Creates scripts that use relative paths so the bundle can be moved anywhere.
pub fn generate_portable_scripts(ctx: &ScriptContext) -> Result<GeneratedScripts> {
    let cmd = render_cmd(ctx)?;
    let powershell = render_powershell(ctx)?;
    let bash = render_bash(ctx)?;
    let readme = render_readme(ctx)?;

    Ok(GeneratedScripts {
        cmd,
        powershell,
        bash,
        readme: Some(readme),
    })
}

/// Generate activation scripts with absolute paths
///
/// Creates scripts that use absolute paths from the provided context.
pub fn generate_absolute_scripts(ctx: &ScriptContext) -> Result<GeneratedScripts> {
    let cmd = render_cmd(ctx)?;
    let powershell = render_powershell(ctx)?;
    let bash = render_bash(ctx)?;

    Ok(GeneratedScripts {
        cmd,
        powershell,
        bash,
        readme: None,
    })
}

/// Generate a single script for the specified shell
pub fn generate_script(ctx: &ScriptContext, shell: ShellType) -> Result<String> {
    match shell {
        ShellType::Cmd => render_cmd(ctx),
        ShellType::PowerShell => render_powershell(ctx),
        ShellType::Bash => render_bash(ctx),
    }
}

/// Generate a single script with absolute paths (convenience wrapper)
pub fn generate_absolute_script(ctx: &ScriptContext, shell: ShellType) -> Result<String> {
    generate_script(ctx, shell)
}

/// Save scripts to a directory
pub async fn save_scripts(
    scripts: &GeneratedScripts,
    output_dir: &std::path::Path,
    base_name: &str,
) -> Result<()> {
    tokio::fs::create_dir_all(output_dir)
        .await
        .map_err(MsvcKitError::Io)?;

    let cmd_path = output_dir.join(format!("{}.bat", base_name));
    let ps_path = output_dir.join(format!("{}.ps1", base_name));
    let bash_path = output_dir.join(format!("{}.sh", base_name));

    tokio::fs::write(&cmd_path, &scripts.cmd)
        .await
        .map_err(MsvcKitError::Io)?;
    tokio::fs::write(&ps_path, &scripts.powershell)
        .await
        .map_err(MsvcKitError::Io)?;
    tokio::fs::write(&bash_path, &scripts.bash)
        .await
        .map_err(MsvcKitError::Io)?;

    if let Some(readme) = &scripts.readme {
        let readme_path = output_dir.join("README.txt");
        tokio::fs::write(&readme_path, readme)
            .await
            .map_err(MsvcKitError::Io)?;
    }

    Ok(())
}

// ==================== Internal Render Functions ====================

fn render_cmd(ctx: &ScriptContext) -> Result<String> {
    let template = CmdScriptTemplate {
        msvc_version: &ctx.msvc_version,
        sdk_version: &ctx.sdk_version,
        arch: ctx.arch.to_string(),
        host_arch: ctx.host_arch_dir().to_string(),
        target_arch: ctx.target_arch_dir().to_string(),
    };

    let rendered = template
        .render()
        .map_err(|e| MsvcKitError::Other(format!("Failed to render CMD template: {}", e)))?;

    // For absolute scripts, replace BUNDLE_ROOT with actual path
    if !ctx.portable {
        let root = ctx.root_expr(ShellType::Cmd);
        Ok(rendered
            .replace("%BUNDLE_ROOT%", &root)
            .lines()
            .filter(|line| {
                // Remove the BUNDLE_ROOT setup lines for absolute scripts
                !line.contains("set \"BUNDLE_ROOT=%~dp0\"")
                    && !line.contains("if \"%BUNDLE_ROOT:~-1%\"")
                    && !line.contains("Get the directory where this script is located")
                    && !line.contains("Remove trailing backslash")
            })
            .collect::<Vec<_>>()
            .join("\n"))
    } else {
        Ok(rendered)
    }
}

fn render_powershell(ctx: &ScriptContext) -> Result<String> {
    let template = PowerShellScriptTemplate {
        msvc_version: &ctx.msvc_version,
        sdk_version: &ctx.sdk_version,
        arch: ctx.arch.to_string(),
        host_arch: ctx.host_arch_dir().to_string(),
        target_arch: ctx.target_arch_dir().to_string(),
    };

    let rendered = template
        .render()
        .map_err(|e| MsvcKitError::Other(format!("Failed to render PowerShell template: {}", e)))?;

    // For absolute scripts, replace $BundleRoot with actual path
    if !ctx.portable {
        let root = ctx.root_expr(ShellType::PowerShell);
        Ok(rendered
            .replace("$BundleRoot", &format!("\"{}\"", root))
            .lines()
            .filter(|line| {
                // Remove the BundleRoot setup lines for absolute scripts
                !line.contains("$PSScriptRoot")
                    && !line.contains("Get the directory where this script is located")
            })
            .collect::<Vec<_>>()
            .join("\n"))
    } else {
        Ok(rendered)
    }
}

fn render_bash(ctx: &ScriptContext) -> Result<String> {
    let template = BashScriptTemplate {
        msvc_version: &ctx.msvc_version,
        sdk_version: &ctx.sdk_version,
        arch: ctx.arch.to_string(),
        host_arch: ctx.host_arch_dir().to_string(),
        target_arch: ctx.target_arch_dir().to_string(),
    };

    let rendered = template
        .render()
        .map_err(|e| MsvcKitError::Other(format!("Failed to render Bash template: {}", e)))?;

    // For absolute scripts, replace $BUNDLE_ROOT with actual path
    if !ctx.portable {
        let root = ctx.root_expr(ShellType::Bash);
        Ok(rendered
            .replace("$BUNDLE_ROOT", &format!("\"{}\"", root))
            .lines()
            .filter(|line| {
                // Remove the BUNDLE_ROOT/SCRIPT_DIR setup lines for absolute scripts
                !line.contains("SCRIPT_DIR=")
                    && !line.contains("BUNDLE_ROOT=")
                    && !line.contains("wslpath")
                    && !line.contains("Get the directory where this script is located")
                    && !line.contains("Convert to Windows path")
            })
            .collect::<Vec<_>>()
            .join("\n"))
    } else {
        Ok(rendered)
    }
}

fn render_readme(ctx: &ScriptContext) -> Result<String> {
    let template = ReadmeTemplate {
        msvc_version: &ctx.msvc_version,
        sdk_version: &ctx.sdk_version,
        arch: ctx.arch.to_string(),
    };

    template
        .render()
        .map_err(|e| MsvcKitError::Other(format!("Failed to render README template: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_type_detect() {
        let _ = ShellType::detect();
    }

    #[test]
    fn test_script_extension() {
        assert_eq!(ShellType::Cmd.script_extension(), "bat");
        assert_eq!(ShellType::PowerShell.script_extension(), "ps1");
        assert_eq!(ShellType::Bash.script_extension(), "sh");
    }

    #[test]
    fn test_script_filename() {
        assert_eq!(ShellType::Cmd.script_filename("setup"), "setup.bat");
        assert_eq!(ShellType::PowerShell.script_filename("setup"), "setup.ps1");
        assert_eq!(ShellType::Bash.script_filename("setup"), "setup.sh");
    }

    #[test]
    fn test_portable_script_context() {
        let ctx = ScriptContext::portable(
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X64,
            Architecture::X64,
        );

        assert!(ctx.portable);
        assert!(ctx.root.is_none());
        assert_eq!(ctx.host_arch_dir(), "Hostx64");
        assert_eq!(ctx.target_arch_dir(), "x64");
    }

    #[test]
    fn test_absolute_script_context() {
        let ctx = ScriptContext::absolute(
            PathBuf::from("C:\\msvc-kit"),
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X64,
            Architecture::X64,
        );

        assert!(!ctx.portable);
        assert!(ctx.root.is_some());
        assert_eq!(ctx.root_expr(ShellType::Cmd), "C:\\msvc-kit");
        assert_eq!(ctx.root_expr(ShellType::PowerShell), "C:\\msvc-kit");
        assert_eq!(ctx.root_expr(ShellType::Bash), "/c/msvc-kit");
    }

    #[test]
    fn test_generate_portable_scripts() {
        let ctx = ScriptContext::portable(
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X64,
            Architecture::X64,
        );

        let scripts = generate_portable_scripts(&ctx).unwrap();

        assert!(scripts.cmd.contains("BUNDLE_ROOT"));
        assert!(scripts.cmd.contains("14.44.34823"));
        assert!(scripts.powershell.contains("$PSScriptRoot"));
        assert!(scripts.bash.contains("BASH_SOURCE"));
        assert!(scripts.readme.is_some());
    }

    #[test]
    fn test_generate_absolute_scripts() {
        let ctx = ScriptContext::absolute(
            PathBuf::from("C:\\msvc-kit"),
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X64,
            Architecture::X64,
        );

        let scripts = generate_absolute_scripts(&ctx).unwrap();

        // Should contain the actual path, not BUNDLE_ROOT
        assert!(scripts.cmd.contains("C:\\msvc-kit"));
        assert!(!scripts.cmd.contains("%BUNDLE_ROOT%"));
        assert!(scripts.powershell.contains("C:\\msvc-kit"));
        assert!(!scripts.powershell.contains("$PSScriptRoot"));
        // Bash should have Unix-style path
        assert!(scripts.bash.contains("/c/msvc-kit"));
        assert!(scripts.readme.is_none());
    }

    #[test]
    fn test_shell_type_display() {
        assert_eq!(format!("{}", ShellType::Cmd), "cmd");
        assert_eq!(format!("{}", ShellType::PowerShell), "powershell");
        assert_eq!(format!("{}", ShellType::Bash), "bash");
    }

    #[test]
    fn test_generated_scripts_get() {
        let scripts = GeneratedScripts {
            cmd: "cmd content".to_string(),
            powershell: "ps content".to_string(),
            bash: "bash content".to_string(),
            readme: Some("readme content".to_string()),
        };

        assert_eq!(scripts.get(ShellType::Cmd), "cmd content");
        assert_eq!(scripts.get(ShellType::PowerShell), "ps content");
        assert_eq!(scripts.get(ShellType::Bash), "bash content");
    }

    #[test]
    fn test_generate_script_single() {
        let ctx = ScriptContext::portable(
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X64,
            Architecture::X64,
        );

        let cmd_script = generate_script(&ctx, ShellType::Cmd).unwrap();
        assert!(cmd_script.contains("14.44.34823"));
        assert!(cmd_script.contains("10.0.26100.0"));

        let ps_script = generate_script(&ctx, ShellType::PowerShell).unwrap();
        assert!(ps_script.contains("14.44.34823"));

        let bash_script = generate_script(&ctx, ShellType::Bash).unwrap();
        assert!(bash_script.contains("14.44.34823"));
    }

    #[test]
    fn test_generate_absolute_script_single() {
        let ctx = ScriptContext::absolute(
            PathBuf::from("C:\\test"),
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X64,
            Architecture::X64,
        );

        let script = generate_absolute_script(&ctx, ShellType::Cmd).unwrap();
        assert!(script.contains("C:\\test"));
    }

    #[test]
    fn test_portable_root_expr() {
        let ctx = ScriptContext::portable(
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X64,
            Architecture::X64,
        );

        assert_eq!(ctx.root_expr(ShellType::Cmd), "%BUNDLE_ROOT%");
        assert_eq!(ctx.root_expr(ShellType::PowerShell), "$BundleRoot");
        assert_eq!(ctx.root_expr(ShellType::Bash), "$BUNDLE_ROOT");
    }

    #[test]
    fn test_script_context_arm64() {
        let ctx = ScriptContext::portable(
            "14.44.34823",
            "10.0.26100.0",
            Architecture::Arm64,
            Architecture::X64,
        );

        assert_eq!(ctx.host_arch_dir(), "Hostx64");
        assert_eq!(ctx.target_arch_dir(), "arm64");
    }

    #[test]
    fn test_script_context_x86() {
        let ctx = ScriptContext::portable(
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X86,
            Architecture::X86,
        );

        assert_eq!(ctx.host_arch_dir(), "Hostx86");
        assert_eq!(ctx.target_arch_dir(), "x86");
    }

    #[test]
    fn test_d_drive_path_conversion() {
        let ctx = ScriptContext::absolute(
            PathBuf::from("D:\\msvc-kit"),
            "14.44.34823",
            "10.0.26100.0",
            Architecture::X64,
            Architecture::X64,
        );

        assert_eq!(ctx.root_expr(ShellType::Bash), "/d/msvc-kit");
    }

    #[tokio::test]
    async fn test_save_scripts() {
        let temp_dir = tempfile::tempdir().unwrap();
        let scripts = GeneratedScripts {
            cmd: "@echo off\necho test".to_string(),
            powershell: "Write-Host 'test'".to_string(),
            bash: "#!/bin/bash\necho test".to_string(),
            readme: Some("README content".to_string()),
        };

        save_scripts(&scripts, temp_dir.path(), "setup")
            .await
            .unwrap();

        // Verify files were created
        assert!(temp_dir.path().join("setup.bat").exists());
        assert!(temp_dir.path().join("setup.ps1").exists());
        assert!(temp_dir.path().join("setup.sh").exists());
        assert!(temp_dir.path().join("README.txt").exists());

        // Verify content
        let cmd_content = std::fs::read_to_string(temp_dir.path().join("setup.bat")).unwrap();
        assert!(cmd_content.contains("echo test"));

        let ps_content = std::fs::read_to_string(temp_dir.path().join("setup.ps1")).unwrap();
        assert!(ps_content.contains("Write-Host"));

        let bash_content = std::fs::read_to_string(temp_dir.path().join("setup.sh")).unwrap();
        assert!(bash_content.contains("#!/bin/bash"));

        let readme_content = std::fs::read_to_string(temp_dir.path().join("README.txt")).unwrap();
        assert!(readme_content.contains("README content"));
    }

    #[tokio::test]
    async fn test_save_scripts_without_readme() {
        let temp_dir = tempfile::tempdir().unwrap();
        let scripts = GeneratedScripts {
            cmd: "cmd".to_string(),
            powershell: "ps".to_string(),
            bash: "bash".to_string(),
            readme: None,
        };

        save_scripts(&scripts, temp_dir.path(), "activate")
            .await
            .unwrap();

        assert!(temp_dir.path().join("activate.bat").exists());
        assert!(temp_dir.path().join("activate.ps1").exists());
        assert!(temp_dir.path().join("activate.sh").exists());
        assert!(!temp_dir.path().join("README.txt").exists());
    }

    #[tokio::test]
    async fn test_save_scripts_creates_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested_dir = temp_dir.path().join("nested").join("dir");

        let scripts = GeneratedScripts {
            cmd: "cmd".to_string(),
            powershell: "ps".to_string(),
            bash: "bash".to_string(),
            readme: None,
        };

        save_scripts(&scripts, &nested_dir, "setup").await.unwrap();

        assert!(nested_dir.join("setup.bat").exists());
    }
}
