# CI/CD Workflows

## Workflows

### Rust (`build_test.yml`)

Runs on every push and pull request to `main`. Builds the project and runs the test suite on Ubuntu.

**Trigger:** Automatic on push/PR to `main`.

### Release (`release.yml`)

Builds cross-platform binaries and creates a GitHub release with all assets attached.

**Trigger:** Manual (`workflow_dispatch`).

**Targets:**
| Platform | Target | Archive |
|----------|--------|---------|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | tar.gz |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | tar.gz |
| macOS Intel | `x86_64-apple-darwin` | tar.gz |
| macOS Apple Silicon | `aarch64-apple-darwin` | tar.gz |
| Windows x86_64 | `x86_64-pc-windows-msvc` | zip |

**Jobs:**
1. **validate** — Checks the tag format, ensures it doesn't already exist, and pushes it.
2. **build** — Compiles release binaries for all 5 targets in parallel and packages them with LICENSE and README.
3. **release** — Downloads all artifacts, generates SHA256 checksums and a changelog, then creates the GitHub release.

**Inputs:**
| Input | Required | Description |
|-------|----------|-------------|
| `tag` | Yes | Release tag, e.g. `v0.2.0` |
| `prerelease` | No | Mark as pre-release (default: false) |
| `notes` | No | Additional release notes |

#### How to run

1. Go to the repository on GitHub.
2. Click the **Actions** tab.
3. Select **Release** from the workflow list on the left sidebar.
4. Click the **Run workflow** dropdown (top right).
5. Fill in the inputs:
   - **tag**: the version to release (e.g. `v0.2.0`). Must follow `v*.*.*` format.
   - **prerelease**: check this for release candidates or beta versions.
   - **notes**: any extra context you want in the release description.
6. Click **Run workflow**.

The workflow will create the git tag, build all binaries, and publish the release automatically. Once complete, the release page will have downloadable archives for all platforms along with SHA256 checksums.
