use std::{env, path::PathBuf, process::Command};

fn main() {
    let task = env::args().nth(1).unwrap_or_else(|| "help".into());
    let result = match task.as_str() {
        "build" => build(),
        "check" => check(),
        "clean" => clean(),
        "install" => install(),
        "lock" => lock(),
        "uninstall" => uninstall(),
        "help" | "--help" | "-h" => {
            help();
            Ok(())
        }
        name => Err(format!("unknown xtask '{name}'")),
    };
    if let Err(message) = result {
        eprintln!("xtask: {message}");
        std::process::exit(1);
    }
}

fn build() -> Result<(), String> {
    build_frontend()?;
    run(
        "cargo",
        &["build", "--locked", "--workspace", "--exclude", "xtask"],
        None,
    )
}

fn clean() -> Result<(), String> {
    run("cargo", &["clean"], None)?;
    for name in ["tsconfig.tsbuildinfo", "tsconfig.app.tsbuildinfo"] {
        let path = root().join("web").join(name);
        match std::fs::remove_file(&path) {
            Ok(()) => println!("Removed {}", path.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to remove {}: {error}", path.display())),
        }
    }
    println!("Preserved web/dist and web/node_modules");
    Ok(())
}

fn install() -> Result<(), String> {
    build_frontend()?;
    run(
        "cargo",
        &["install", "--path", ".", "--force", "--locked"],
        None,
    )?;
    println!(
        "Installed fer at {}",
        cargo_bin().join(executable("fer")).display()
    );
    Ok(())
}

fn lock() -> Result<(), String> {
    run("cargo", &["update", "--workspace"], None)
}

fn uninstall() -> Result<(), String> {
    run("cargo", &["uninstall", "ferricket"], None)?;
    println!("Removed fer from {}", cargo_bin().display());
    Ok(())
}

fn build_frontend() -> Result<(), String> {
    bun(&["install", "--frozen-lockfile"])?;
    bun(&["run", "build"])
}

fn check() -> Result<(), String> {
    bun(&["install", "--frozen-lockfile"])?;
    bun(&["audit"])?;
    bun(&["run", "format:check"])?;
    bun(&["run", "check"])?;
    bun(&["test"])?;
    bun(&["run", "build"])?;
    run("cargo", &["fmt", "--all", "--", "--check"], None)?;
    run(
        "cargo",
        &["test", "--locked", "--workspace", "--all-targets"],
        None,
    )?;
    run(
        "cargo",
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        None,
    )
}

fn bun(arguments: &[&str]) -> Result<(), String> {
    run("bun", arguments, Some(root().join("web")))
}

fn run(command: &str, arguments: &[&str], directory: Option<PathBuf>) -> Result<(), String> {
    println!("+ {command} {}", arguments.join(" "));
    let mut process = Command::new(command);
    process.args(arguments);
    process.current_dir(directory.unwrap_or_else(root));
    let status = process
        .status()
        .map_err(|error| format!("failed to run {command}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command} exited with {status}"))
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be inside the workspace")
        .to_owned()
}

fn cargo_bin() -> PathBuf {
    env::var_os("CARGO_INSTALL_ROOT")
        .map(PathBuf::from)
        .or_else(|| env::var_os("CARGO_HOME").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .unwrap_or_else(|| PathBuf::from(".cargo"))
        .join("bin")
}

fn executable(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn help() {
    println!("cargo x build  Build the Bun frontend and Rust CLI");
    println!("cargo x check  Run frontend and Rust verification");
    println!("cargo x clean  Remove local build artifacts");
    println!("cargo x install  Build and install fer into Cargo's local bin directory");
    println!("cargo x lock  Refresh workspace package versions in Cargo.lock");
    println!("cargo x uninstall  Remove fer from Cargo's local bin directory");
    println!("(cargo xtask also works)");
}
