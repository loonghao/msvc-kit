Portable MSVC Toolchain Bundle
==============================

MSVC Version:        {{ msvc_version }}
Windows SDK Version: {{ sdk_version }}
Architecture:        {{ arch }}

Contents:
- setup.bat        : CMD activation script
- setup.ps1        : PowerShell activation script
- setup.sh         : Bash/WSL activation script
- VC/              : Visual C++ compiler and libraries
- Windows Kits/    : Windows SDK

Usage:
1. Extract this bundle to your desired location
2. Run the appropriate setup script for your shell:
   - CMD:        setup.bat
   - PowerShell: .\setup.ps1
   - Bash/WSL:   source setup.sh
3. cl, link, nmake, and other MSVC tools become available

Directory Structure:
- VC/Tools/MSVC/{{ msvc_version }}/bin/...  : Compiler binaries (cl.exe, link.exe)
- VC/Tools/MSVC/{{ msvc_version }}/include/ : C++ headers
- VC/Tools/MSVC/{{ msvc_version }}/lib/     : Static libraries
- Windows Kits/10/Include/  : Windows SDK headers
- Windows Kits/10/Lib/      : Windows SDK libraries
- Windows Kits/10/bin/      : SDK tools (rc.exe)

License Notice:
The MSVC compiler and Windows SDK included in this bundle are
property of Microsoft and subject to Microsoft Visual Studio
License Terms: https://visualstudio.microsoft.com/license-terms/

This bundle was created for personal/development use. Microsoft
software components are NOT covered by msvc-kit's MIT license.
