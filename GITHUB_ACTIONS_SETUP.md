# GitHub Actions Setup Guide

This guide explains how to enable and use the automated build system with GitHub Actions.

## Current Status

⚠️ **GitHub Actions is configured but not active yet**

The workflow file exists at `.github/workflows/build.yml` but won't run until you:
1. Commit the changes to your repository
2. Push to GitHub
3. The workflow will then run automatically

## Quick Start

### 1. Commit All Changes

```bash
# Check what's been modified
git status

# Add all new and modified files
git add .

# Commit with a message
git commit -m "Add cross-platform build system with file associations"
```

### 2. Push to GitHub

```bash
# Push to main branch
git push origin main
```

✅ **GitHub Actions will now run automatically!**

## What GitHub Actions Will Do

### On Every Push to `main`

The workflow will automatically:

1. **Build Linux Version**
   - Install dependencies
   - Build with PyInstaller
   - Create standalone binary
   - Create .deb package
   - Create portable .tar.gz
   - Upload artifacts

2. **Build Windows Version**
   - Install dependencies
   - Build with PyInstaller
   - Create portable ZIP
   - Upload artifacts

3. **Store Build Artifacts**
   - Available for 90 days
   - Download from Actions tab

### On Git Tags (Releases)

When you create a version tag:

```bash
git tag -a v0.2.0 -m "Release version 0.2.0"
git push origin v0.2.0
```

GitHub Actions will:
1. Build for Linux and Windows
2. **Create a GitHub Release**
3. **Upload all distribution files**:
   - `ParVu-0.2.0-amd64.deb`
   - `ParVu-0.2.0-linux-x86_64.tar.gz`
   - `ParVu-0.2.0-portable-win64.zip`

## Viewing Build Results

### Check Workflow Status

1. Go to your GitHub repository
2. Click **Actions** tab
3. See build status for each push

### Download Build Artifacts

1. Go to **Actions** tab
2. Click on a completed workflow run
3. Scroll to **Artifacts** section
4. Download:
   - `parvu-linux-binary`
   - `parvu-linux-deb`
   - `parvu-linux-portable`
   - `parvu-windows-binary`
   - `parvu-windows-portable`

## Workflow Triggers

The workflow runs on:

```yaml
on:
  push:
    branches: [ main ]          # Every push to main
  pull_request:
    branches: [ main ]          # Every pull request
  tags:
    - 'v*'                      # Version tags (v0.2.0, v1.0.0, etc.)
  workflow_dispatch:            # Manual trigger
```

### Manual Trigger

You can also run the workflow manually:

1. Go to **Actions** tab
2. Click **Build ParVu** workflow
3. Click **Run workflow** button
4. Select branch
5. Click **Run workflow**

## Creating a Release

### Recommended Release Process

```bash
# 1. Update version in pyproject.toml
# Edit pyproject.toml: version = "0.3.0"

# 2. Update CHANGELOG.md
# Document changes

# 3. Commit version bump
git add pyproject.toml CHANGELOG.md
git commit -m "Bump version to 0.3.0"

# 4. Create annotated tag
git tag -a v0.3.0 -m "Release version 0.3.0

Features:
- Cross-platform build system
- File associations support
- Improved documentation

See CHANGELOG.md for details."

# 5. Push commits and tag
git push origin main
git push origin v0.3.0

# GitHub Actions will now:
# - Build for Linux and Windows
# - Create GitHub Release
# - Upload all distribution files
```

### Release Will Include

- **Linux .deb package** with file associations
- **Linux portable archive** (.tar.gz)
- **Windows portable** (.zip)
- **Release notes** (auto-generated from commits)

## Troubleshooting

### "Workflow was skipped"

**Cause**: Changes not pushed to GitHub

**Solution**:
```bash
git add .
git commit -m "Your message"
git push origin main
```

### Build Fails on GitHub

**Check the logs**:
1. Go to Actions tab
2. Click failed workflow
3. Click failed job
4. Expand steps to see error

**Common issues**:

1. **Missing `parvu.spec`**
   - The build script auto-creates it
   - Ensure `build.sh` is included

2. **Dependency issues**
   - Check `pyproject.toml` is correct
   - Verify `[project.optional-dependencies]` section exists

3. **Build timeout**
   - Builds typically take 3-5 minutes
   - If >60 minutes, something is wrong

### Artifacts Not Uploading

**Check**:
- Artifact paths in workflow match build output
- Build completed successfully
- No file size limits exceeded (artifacts <2GB each)

### Release Not Created

**Verify**:
- Tag starts with `v` (e.g., `v0.2.0` not `0.2.0`)
- Tag was pushed: `git push origin v0.2.0`
- Both Linux and Windows builds succeeded

## Workflow Configuration

### Current Jobs

```yaml
jobs:
  build-linux:         # Builds .deb and .tar.gz
  build-windows:       # Builds portable .zip
  release:             # Creates GitHub Release (tags only)
```

### Build Matrix (Future Enhancement)

To build for multiple Python versions:

```yaml
strategy:
  matrix:
    python-version: ['3.11', '3.12', '3.13']
```

## Customization

### Change Workflow Name

Edit `.github/workflows/build.yml`:
```yaml
name: Build ParVu  # Change this
```

### Add More Build Steps

```yaml
- name: Run tests
  run: pytest tests/

- name: Code quality check
  run: ruff check src/
```

### Change Trigger Branches

```yaml
on:
  push:
    branches: [ main, develop ]  # Add more branches
```

## Secrets and Tokens

### GITHUB_TOKEN

✅ **Already configured** - No action needed

GitHub automatically provides `GITHUB_TOKEN` for:
- Creating releases
- Uploading artifacts
- Accessing repository

### Code Signing (Future)

For Windows code signing, you'll need to add secrets:

1. Go to **Settings** → **Secrets and variables** → **Actions**
2. Click **New repository secret**
3. Add:
   - `WINDOWS_CERTIFICATE` (base64-encoded .pfx file)
   - `CERTIFICATE_PASSWORD`

## Best Practices

### 1. Use Semantic Versioning

```
v0.2.0 - Initial release
v0.2.1 - Patch (bug fixes)
v0.3.0 - Minor (new features)
v1.0.0 - Major (breaking changes)
```

### 2. Test Before Tagging

```bash
# Push to branch first
git push origin main

# Wait for Actions to complete
# Verify builds succeed

# Then create tag
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin v0.2.0
```

### 3. Write Good Release Notes

Include in tag message:
- What's new
- What's fixed
- Breaking changes
- Migration guide (if needed)

### 4. Keep Artifacts Organized

- Use consistent naming
- Include version in filenames
- Separate Linux/Windows artifacts

## Monitoring

### Build Status Badge

Add to README.md:

```markdown
![Build Status](https://github.com/AzizNadirov/ParVu/actions/workflows/build.yml/badge.svg)
```

### Email Notifications

GitHub automatically sends emails for:
- Failed builds (to commit author)
- Completed workflow runs (configurable)

**Configure notifications**:
1. Go to your **Settings** (personal)
2. **Notifications**
3. **Actions** section
4. Choose notification preferences

## Advanced Features

### Caching Dependencies

Speed up builds by caching uv dependencies:

```yaml
- name: Cache uv
  uses: actions/cache@v3
  with:
    path: ~/.cache/uv
    key: ${{ runner.os }}-uv-${{ hashFiles('**/pyproject.toml') }}
```

### Build Matrix

Build for multiple platforms simultaneously:

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, windows-latest, macos-latest]
runs-on: ${{ matrix.os }}
```

### Conditional Steps

```yaml
- name: Upload to PyPI
  if: startsWith(github.ref, 'refs/tags/v')
  run: |
    python -m build
    twine upload dist/*
```

## Next Steps

1. **Commit and push current changes**
   ```bash
   git add .
   git commit -m "Add build system and documentation"
   git push origin main
   ```

2. **Verify Actions run successfully**
   - Check Actions tab on GitHub
   - Download and test artifacts

3. **Create first release**
   ```bash
   git tag -a v0.2.0 -m "First release with build system"
   git push origin v0.2.0
   ```

4. **Announce release**
   - GitHub Releases page
   - Social media
   - Documentation

## Support

- **GitHub Actions Docs**: https://docs.github.com/en/actions
- **Workflow Syntax**: https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions
- **ParVu Issues**: https://github.com/AzizNadirov/ParVu/issues

---

## Summary

✅ **Workflow configured** - Ready to use
⏳ **Waiting for**: Commit and push to activate
🚀 **Will automatically**: Build on every push, create releases on tags

**To activate now:**
```bash
git add .
git commit -m "Add cross-platform build system"
git push origin main
```

Then visit: `https://github.com/AzizNadirov/ParVu/actions` to see it running!
