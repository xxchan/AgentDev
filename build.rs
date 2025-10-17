use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use cargo_emit::{rerun_if_changed, rerun_if_env_changed, rustc_cfg};

const FRONTEND_DIR: &str = "apps/frontend";
const FRONTEND_DIST_DIRS: &[&str] = &[".next-prod", "out"];
const ENV_SKIP_BUILD: &str = "AGENTDEV_SKIP_UI_BUILD";

fn env_var_truthy(key: &str) -> bool {
    rerun_if_env_changed!(key);

    env::var(key)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "t" | "yes" | "y"
            )
        })
        .unwrap_or(false)
}

fn dest_dir() -> Result<PathBuf> {
    let out_dir =
        env::var("OUT_DIR").context("OUT_DIR environment variable missing for build script")?;
    Ok(Path::new(&out_dir).join("assets"))
}

fn track_frontend_sources() {
    const FILES: &[&str] = &[
        "package.json",
        "package-lock.json",
        "pnpm-lock.yaml",
        "tsconfig.json",
        "next.config.ts",
        "postcss.config.mjs",
        "tailwind.config.ts",
        "components.json",
    ];
    const DIRS: &[&str] = &[
        "app",
        "components",
        "hooks",
        "lib",
        "public",
        "styles",
        "types",
    ];

    for file in FILES {
        rerun_if_changed!(format!("{FRONTEND_DIR}/{file}"));
    }
    for dir in DIRS {
        rerun_if_changed!(format!("{FRONTEND_DIR}/{dir}"));
    }
}

fn pnpm_command() -> String {
    env::var("AGENTDEV_PNPM_BIN")
        .or_else(|_| env::var("PNPM_BIN"))
        .unwrap_or_else(|_| "pnpm".to_string())
}

fn run_pnpm(args: &[&str]) -> Result<()> {
    let status = Command::new(pnpm_command())
        .args(args)
        .current_dir(FRONTEND_DIR)
        .status()
        .with_context(|| format!("failed to execute pnpm command {args:?}"))?;

    if !status.success() {
        bail!("pnpm command {args:?} exited with status {status:?}");
    }

    Ok(())
}

fn build_frontend_assets() -> Result<()> {
    let frontend_path = Path::new(FRONTEND_DIR);
    if !frontend_path.exists() {
        bail!(
            "frontend directory '{}' is missing; ensure repository submodules are checked out",
            frontend_path.display()
        );
    }

    run_pnpm(&["install", "--frozen-lockfile"])
        .context("failed to install frontend dependencies with pnpm")?;

    run_pnpm(&["run", "build"]).context("failed to build frontend bundle")?;

    let dist = find_frontend_dist(frontend_path)?;

    let destination = dest_dir()?;
    if destination.exists() {
        std::fs::remove_dir_all(&destination).with_context(|| {
            format!(
                "failed to clear previous assets at {}",
                destination.display()
            )
        })?;
    }
    std::fs::create_dir_all(&destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    dircpy::copy_dir(&dist, &destination)
        .with_context(|| format!("failed to copy assets into {}", destination.display()))?;

    Ok(())
}

fn find_frontend_dist(frontend_path: &Path) -> Result<PathBuf> {
    for dir in FRONTEND_DIST_DIRS {
        let candidate = frontend_path.join(dir);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!(
        "frontend build completed but none of the expected output directories were found ({})",
        FRONTEND_DIST_DIRS.join(", ")
    );
}

fn main() -> Result<()> {
    println!("cargo::rustc-check-cfg=cfg(agentdev_ui_built)");
    track_frontend_sources();

    if env_var_truthy(ENV_SKIP_BUILD) {
        let destination = dest_dir()?;
        std::fs::create_dir_all(&destination).with_context(|| {
            format!(
                "failed to create placeholder asset directory at {}",
                destination.display()
            )
        })?;
        return Ok(());
    }

    build_frontend_assets()?;
    rustc_cfg!("agentdev_ui_built");

    Ok(())
}
