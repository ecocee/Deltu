# DELTU AI Installation Prompt

Copy and paste the following prompt into your favorite AI coding assistant (Codex, Claude Code, Cursor, Gemini CLI, etc.) to have it autonomously install and verify DELTU in your environment.

```text
Please install the DELTU event-processing engine in my environment.

DELTU is a lightweight, local-first event processor.
Repository: https://github.com/ecocee/Deltu

1. Inspect my OS and CPU architecture.
2. The official installation script is at: https://raw.githubusercontent.com/ecocee/Deltu/main/scripts/install.sh
3. Run the installer safely: `curl -fsSL https://raw.githubusercontent.com/ecocee/Deltu/main/scripts/install.sh | sh`
4. The installer places the binary at `~/.deltu/bin/deltu`. Ensure this path is added to my shell's PATH, or use the absolute path for the next steps.
5. Verify the installation by running `deltu version` (or `~/.deltu/bin/deltu version`).
6. Run `deltu doctor` to ensure the system is ready.
7. Run `deltu demo` to verify the engine works.
8. If the installation fails due to an unsupported architecture, try building from source by cloning the repository and running `cargo build --release`.
9. Report back the final status, the version installed, and output of the demo. Do not invent release URLs or binaries.
```
