# Release Command

Create a new release for msvc-kit.

## Usage
```
/release <version>
```

## Arguments
- `version`: Version number (e.g., 0.1.0)

## Actions
1. Update version in Cargo.toml
2. Run tests to ensure everything works
3. Create git tag
4. Push to trigger CI release workflow
5. Remind to update winget manifest

## Example
```
/release 0.2.0
```
