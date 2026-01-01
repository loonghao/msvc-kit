# Build Command

Build the msvc-kit project.

## Usage
```
/build [mode]
```

## Arguments
- `mode`: Build mode (debug/release), default: debug

## Actions
1. Run `cargo build` or `cargo build --release`
2. Report build status and any errors
3. Show binary location on success

## Example
```
/build release
```
