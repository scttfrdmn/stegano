# Development Guidelines

This document outlines the development practices, versioning standards, and release procedures for the Stegano project.

## Table of Contents

- [Semantic Versioning](#semantic-versioning)
- [Changelog Management](#changelog-management)
- [Release Process](#release-process)
- [Code Quality Standards](#code-quality-standards)
- [Git Workflow](#git-workflow)
- [Documentation Standards](#documentation-standards)

## Semantic Versioning

This project strictly adheres to [Semantic Versioning 2.0.0](https://semver.org/).

### Version Format: MAJOR.MINOR.PATCH

#### MAJOR version (X.y.z)
Increment when making **incompatible API changes**:
- Breaking changes to public API
- Removal of deprecated features
- Major architecture changes that affect user code
- Changes that require user code modifications

**Examples:**
- Removing a public function or struct
- Changing function signatures
- Major refactoring that changes core interfaces
- Moving from synchronous to asynchronous APIs

#### MINOR version (x.Y.z)
Increment when adding **functionality in a backwards compatible manner**:
- New features that don't break existing code
- New public APIs
- New optional parameters with defaults
- Performance improvements
- New data format support

**Examples:**
- Adding new embedding algorithms
- New CLI commands or flags
- Additional validation methods
- New GPU backends
- Enhanced statistical tests

#### PATCH version (x.y.Z)
Increment when making **backwards compatible bug fixes**:
- Security fixes
- Bug fixes that don't change API
- Documentation improvements
- Performance optimizations without API changes
- Dependency updates

**Examples:**
- Fixing memory leaks
- Correcting calculation errors
- Improving error messages
- Security vulnerability patches

### Pre-release Versions

For development and testing versions, use pre-release identifiers:
- `1.0.0-alpha.1` - Alpha releases for early testing
- `1.0.0-beta.1` - Beta releases for feature-complete testing
- `1.0.0-rc.1` - Release candidates for final testing

### Version Decision Matrix

| Change Type | API Break | New Features | Bug Fix | Version Impact |
|-------------|-----------|--------------|---------|----------------|
| Remove function | ✅ | ❌ | ❌ | MAJOR |
| Change function signature | ✅ | ❌ | ❌ | MAJOR |
| Add new function | ❌ | ✅ | ❌ | MINOR |
| Add optional parameter | ❌ | ✅ | ❌ | MINOR |
| Fix bug | ❌ | ❌ | ✅ | PATCH |
| Security fix | ❌ | ❌ | ✅ | PATCH |
| Performance improvement | ❌ | ✅/❌ | ❌ | MINOR/PATCH |

## Changelog Management

We follow [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) format.

### Required Sections

Every release must include these sections (even if empty):

```markdown
## [Version] - YYYY-MM-DD

### Added
- New features

### Changed
- Changes in existing functionality

### Deprecated
- Soon-to-be removed features

### Removed
- Now removed features

### Fixed
- Bug fixes

### Security
- Vulnerability fixes
```

### Writing Guidelines

#### Added Section
- **New features**: Complete new functionality
- **New APIs**: Public functions, structs, or modules
- **New format support**: Additional data formats
- **New algorithms**: ML models, embedding techniques
- **New tools**: CLI commands, scripts, utilities

#### Changed Section
- **API modifications**: Non-breaking changes to existing APIs
- **Behavior changes**: Different behavior with same interface
- **Performance improvements**: Speed or memory optimizations
- **Default changes**: Modified default values or behaviors
- **Dependency updates**: Major dependency version bumps

#### Deprecated Section
- **Future removal warnings**: Features planned for removal
- **Alternative recommendations**: What to use instead
- **Timeline**: When removal is planned
- **Migration guides**: How to update code

#### Removed Section
- **Deleted features**: Features no longer available
- **API removals**: Functions or modules removed
- **Dependency removals**: No longer supported libraries
- **Breaking changes**: What was removed and why

#### Fixed Section
- **Bug fixes**: Corrected behavior
- **Memory leaks**: Fixed resource issues
- **Error handling**: Improved error messages
- **Edge cases**: Fixed corner case bugs
- **Compilation issues**: Fixed build problems

#### Security Section
- **Vulnerability fixes**: CVE resolutions
- **Security improvements**: Enhanced protection
- **Cryptographic updates**: Algorithm improvements
- **Access control**: Permission or authentication fixes

### Changelog Rules

1. **One line per change**: Keep entries concise and focused
2. **User-facing language**: Write for users, not developers
3. **Link to issues**: Reference GitHub issues when relevant
4. **Be specific**: Avoid vague terms like "various fixes"
5. **Group related changes**: Combine similar changes into bullet points
6. **Link comparisons**: Include GitHub compare links at bottom

### Example Entry

```markdown
## [0.3.1] - 2025-01-15

### Added
- Statistical validation export to JSON and CSV formats (#123)
- GPU memory usage monitoring for Metal backend (#125)
- CLI progress bars for long-running operations (#127)

### Changed
- Improved chi-square test performance by 25% (#124)
- Enhanced error messages for invalid file formats (#126)
- Updated dependency: `wgpu` from 22.0 to 22.1 (#128)

### Fixed
- Memory leak in GPU texture cleanup (#129)
- Incorrect capacity calculation for DICOM files (#130)
- CLI crash when analyzing empty files (#131)

### Security
- Updated `Blake3` dependency to fix potential timing attack (#132)
```

## Release Process

### Pre-release Checklist

1. **Version Planning**
   - [ ] Determine version increment (major/minor/patch)
   - [ ] Review all changes since last release
   - [ ] Verify no breaking changes in minor/patch releases

2. **Code Quality**
   - [ ] All tests pass (`cargo test`)
   - [ ] Code coverage meets minimum 85%
   - [ ] Security audit passes (`cargo audit`)
   - [ ] License compliance verified (`cargo deny check`)
   - [ ] No compiler warnings in release mode

3. **Documentation**
   - [ ] Changelog updated with all changes
   - [ ] API documentation current (`cargo doc`)
   - [ ] README reflects new features
   - [ ] Security policy updated if needed

4. **Testing**
   - [ ] Integration tests pass
   - [ ] Performance benchmarks stable
   - [ ] Cross-platform testing complete
   - [ ] GPU acceleration tested on available platforms

### Release Steps

1. **Update Version**
   ```bash
   # Update Cargo.toml version
   vim Cargo.toml
   
   # Update changelog with release date
   vim CHANGELOG.md
   
   # Commit version bump
   git add Cargo.toml CHANGELOG.md
   git commit -m "chore: bump version to X.Y.Z"
   ```

2. **Create Release**
   ```bash
   # Create and push tag
   git tag -a vX.Y.Z -m "Release version X.Y.Z"
   git push origin vX.Y.Z
   
   # Create GitHub release
   gh release create vX.Y.Z --title "Release X.Y.Z" --notes-file CHANGELOG.md
   ```

3. **Post-release**
   ```bash
   # Update changelog for next development
   # Add new [Unreleased] section
   vim CHANGELOG.md
   
   git add CHANGELOG.md
   git commit -m "chore: prepare changelog for next release"
   git push origin main
   ```

## Code Quality Standards

### Testing Requirements

- **Minimum code coverage**: 85%
- **Integration tests**: Required for all major features
- **Performance tests**: Benchmarks for critical paths
- **Security tests**: Validation of cryptographic components

### Static Analysis

```bash
# Required checks before commit
cargo clippy -- -D warnings           # No clippy warnings
cargo fmt --check                     # Code formatting
cargo audit                          # Security vulnerabilities
cargo deny check                     # License compliance
```

### Performance Standards

- **GPU acceleration**: 10x improvement minimum for supported operations
- **Memory usage**: No unbounded growth in long-running operations
- **Benchmark regression**: Max 5% performance decrease in new releases

## Git Workflow

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only changes
- `style`: Formatting, missing semicolons, etc.
- `refactor`: Code change that neither fixes bug nor adds feature
- `perf`: Performance improvement
- `test`: Adding missing tests
- `chore`: Changes to build process, auxiliary tools

**Examples:**
```
feat(validation): add chi-square statistical test
fix(gpu): resolve memory leak in Metal backend
docs(api): update embedding examples
chore: bump version to 0.3.1
```

### Branch Strategy

- **main**: Production-ready code
- **develop**: Integration branch for features
- **feature/\***: Feature development branches
- **hotfix/\***: Critical bug fixes
- **release/\***: Release preparation branches

### Pull Request Requirements

1. **All tests pass** in CI
2. **Code review** by at least one maintainer
3. **Security review** for cryptographic changes
4. **Performance review** for optimization changes
5. **Documentation updated** for user-facing changes

## Documentation Standards

### Required Documentation

1. **API Documentation**: All public APIs must have rustdoc comments
2. **Examples**: Complex features need usage examples
3. **Performance Notes**: Document performance characteristics
4. **Security Considerations**: Note security implications
5. **Compatibility**: Document version compatibility

### Documentation Format

```rust
/// Brief one-line description
///
/// Longer description explaining the function's purpose,
/// behavior, and important considerations.
///
/// # Arguments
///
/// * `param1` - Description of parameter
/// * `param2` - Description of parameter
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Description of possible errors
///
/// # Examples
///
/// ```rust
/// use stegano::SteganoEmbedder;
/// 
/// let embedder = SteganoEmbedder::new();
/// let result = embedder.embed(&mut carrier, payload, &params).await?;
/// ```
///
/// # Security
///
/// Security considerations and warnings
///
/// # Performance
///
/// Performance characteristics and optimization notes
pub fn example_function(param1: Type1, param2: Type2) -> Result<ReturnType> {
    // Implementation
}
```

### Update Requirements

- **API changes**: Update documentation before merge
- **Breaking changes**: Update migration guides
- **New features**: Add examples and tutorials
- **Performance changes**: Update benchmark documentation

---

## Enforcement

These guidelines are enforced through:

1. **Automated checks** in CI/CD pipeline
2. **Code review** requirements
3. **Release gate** checklist verification
4. **Regular audits** of version consistency

## Questions and Clarifications

For questions about these guidelines, please:

1. Open a GitHub issue with label `documentation`
2. Contact the maintainer team at scttfrdmn@gmail.com
3. Refer to the official SemVer and Keep a Changelog documentation

---

*Last updated: January 12, 2025*  
*Next review: April 12, 2025*