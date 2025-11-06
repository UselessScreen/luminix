# Luminix
Luminix is a simple image viewer that I made because I wanted a plain image viewer that just showed the image and nothing else. You can pan and zoom on the image but that's it. Currently it is not finished so many features are missing.

## Release Process

This project uses automated versioning and releases. When you push to the `master` branch, the version in `Cargo.toml` will be automatically updated based on keywords in your commit message:

- **[major]** or **breaking change** - Increments major version (1.0.0 → 2.0.0)
- **[minor]** or **feat**/**feature** - Increments minor version (1.0.0 → 1.1.0)
- **[patch]** or **fix**/**bugfix** - Increments patch version (1.0.0 → 1.0.1)

After incrementing the version, a GitHub release will be automatically created with the new version tag.

### Example commit messages:
- `[patch] fix: resolve image loading bug` → version 0.1.0 → 0.1.1
- `[minor] feat: add keyboard shortcuts` → version 0.1.1 → 0.2.0
- `[major] breaking change: redesign UI` → version 0.2.0 → 1.0.0
