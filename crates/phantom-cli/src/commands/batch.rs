use std::process::Command;

use anyhow::Result;

/// Execute phantom commands from a file, one per line.
/// Lines starting with # are comments. Empty lines are skipped.
pub async fn execute(file: String) -> Result<()> {
    let content = std::fs::read_to_string(&file)?;
    let exe = std::env::current_exe()?;

    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse the line into args (simple split — no shell quoting)
        let args: Vec<&str> = line.split_whitespace().collect();
        if args.is_empty() {
            continue;
        }

        // If line starts with "phantom" or "pt", skip that first word
        let args = if args[0] == "phantom" || args[0] == "pt" {
            &args[1..]
        } else {
            &args[..]
        };

        let status = Command::new(&exe).args(args).status()?;

        if !status.success() {
            let code = status.code().unwrap_or(1);
            eprintln!("Line {}: command failed with exit code {code}", i + 1);
            eprintln!("  {line}");
            std::process::exit(code);
        }
    }

    Ok(())
}
