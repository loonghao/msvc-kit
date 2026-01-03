# Blender Addon Development

Build Blender C/C++ addons and Python extensions using msvc-kit.

## Requirements

| Blender Version | MSVC Version | Python |
|-----------------|--------------|--------|
| Blender 4.2+ | 14.38 | 3.11 |
| Blender 4.0-4.1 | 14.36 | 3.10 |
| Blender 3.6 LTS | 14.34 | 3.10 |
| Blender 3.3 LTS | 14.32 | 3.10 |

## Setup

### 1. Download MSVC

```bash
# For Blender 4.2+
msvc-kit download --msvc-version 14.38 --sdk-version 10.0.22621.0
```

### 2. Setup Environment

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression
```

## Python C Extension

Most Blender addons are pure Python, but you can write C extensions for performance.

### Extension Source (fast_mesh.c)

```c
#define PY_SSIZE_T_CLEAN
#include <Python.h>
#include <numpy/arrayobject.h>

static PyObject* calculate_normals(PyObject* self, PyObject* args) {
    PyArrayObject* vertices;
    PyArrayObject* faces;
    
    if (!PyArg_ParseTuple(args, "O!O!", 
            &PyArray_Type, &vertices,
            &PyArray_Type, &faces)) {
        return NULL;
    }
    
    // Fast normal calculation here...
    
    Py_RETURN_NONE;
}

static PyMethodDef FastMeshMethods[] = {
    {"calculate_normals", calculate_normals, METH_VARARGS, "Calculate vertex normals"},
    {NULL, NULL, 0, NULL}
};

static struct PyModuleDef fastmeshmodule = {
    PyModuleDef_HEAD_INIT,
    "fast_mesh",
    NULL,
    -1,
    FastMeshMethods
};

PyMODINIT_FUNC PyInit_fast_mesh(void) {
    import_array();
    return PyModule_Create(&fastmeshmodule);
}
```

### Build

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

$BLENDER = "C:\Program Files\Blender Foundation\Blender 4.2"
$PYTHON = "$BLENDER\4.2\python"

cl /c /O2 /MD /EHsc `
   /I"$PYTHON\include" `
   /I"$PYTHON\include\numpy" `
   fast_mesh.c

link /DLL /OUT:fast_mesh.pyd `
     fast_mesh.obj `
     /LIBPATH:"$PYTHON\libs" `
     python311.lib
```

## Blender as a Module

Build scripts that use Blender as a Python module:

### bpy Extension

```cpp
#include <pybind11/pybind11.h>
#include <pybind11/numpy.h>

namespace py = pybind11;

py::array_t<float> process_mesh_data(py::array_t<float> vertices) {
    auto buf = vertices.request();
    float* ptr = static_cast<float*>(buf.ptr);
    
    // Process vertices...
    
    return vertices;
}

PYBIND11_MODULE(mesh_processor, m) {
    m.def("process_mesh_data", &process_mesh_data, "Process mesh vertex data");
}
```

### Build with pybind11

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

$BLENDER = "C:\Program Files\Blender Foundation\Blender 4.2"
$PYTHON = "$BLENDER\4.2\python"

cl /c /O2 /MD /EHsc `
   /I"$PYTHON\include" `
   /I"path\to\pybind11\include" `
   mesh_processor.cpp

link /DLL /OUT:mesh_processor.pyd `
     mesh_processor.obj `
     /LIBPATH:"$PYTHON\libs" `
     python311.lib
```

## CMake Build

### CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.20)
project(BlenderAddon)

set(CMAKE_CXX_STANDARD 17)

# Blender Python
set(BLENDER_ROOT "C:/Program Files/Blender Foundation/Blender 4.2"
    CACHE PATH "Blender installation")
set(BLENDER_PYTHON "${BLENDER_ROOT}/4.2/python")

find_package(pybind11 REQUIRED)

# Extension module
pybind11_add_module(mesh_processor
    src/mesh_processor.cpp
)

target_include_directories(mesh_processor PRIVATE
    ${BLENDER_PYTHON}/include
)

target_link_directories(mesh_processor PRIVATE
    ${BLENDER_PYTHON}/libs
)
```

### Build

```powershell
msvc-kit setup --script --shell powershell | Invoke-Expression

cmake -B build -G "NMake Makefiles" -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

## Addon Structure

```
my_addon/
├── __init__.py
├── operators.py
├── ui.py
└── native/
    ├── fast_mesh.pyd
    └── __init__.py
```

### __init__.py

```python
bl_info = {
    "name": "My Addon",
    "blender": (4, 2, 0),
    "category": "Mesh",
}

from . import operators, ui
from .native import fast_mesh

def register():
    operators.register()
    ui.register()

def unregister():
    operators.unregister()
    ui.unregister()
```

## Library API Usage

```rust
use msvc_kit::{download_msvc, download_sdk, setup_environment, DownloadOptions};
use std::process::Command;

async fn build_blender_extension() -> msvc_kit::Result<()> {
    let options = DownloadOptions {
        msvc_version: Some("14.38".to_string()),
        ..Default::default()
    };
    
    let msvc = download_msvc(&options).await?;
    let sdk = download_sdk(&options).await?;
    let env = setup_environment(&msvc, Some(&sdk))?;
    
    std::env::set_var("INCLUDE", env.include_path_string());
    std::env::set_var("LIB", env.lib_path_string());
    
    let blender_python = "C:\\Program Files\\Blender Foundation\\Blender 4.2\\4.2\\python";
    
    let cl = env.cl_exe_path().expect("cl.exe not found");
    Command::new(&cl)
        .args([
            "/c", "/O2", "/MD", "/EHsc",
            &format!("/I{}\\include", blender_python),
            "fast_mesh.c"
        ])
        .status()?;
    
    let link = env.link_exe_path().expect("link.exe not found");
    Command::new(&link)
        .args([
            "/DLL", "/OUT:fast_mesh.pyd",
            "fast_mesh.obj",
            &format!("/LIBPATH:{}\\libs", blender_python),
            "python311.lib"
        ])
        .status()?;
    
    Ok(())
}
```

## Installation

Copy the addon to Blender's addon directory:

```powershell
$ADDON_DIR = "$env:APPDATA\Blender Foundation\Blender\4.2\scripts\addons"
Copy-Item -Recurse my_addon "$ADDON_DIR\"
```

## Troubleshooting

### Import Error

Ensure the .pyd file matches Blender's Python version:

```python
import sys
print(sys.version)  # Should match your build
```

### NumPy Not Found

Install NumPy in Blender's Python:

```powershell
& "C:\Program Files\Blender Foundation\Blender 4.2\4.2\python\bin\python.exe" -m pip install numpy
```
