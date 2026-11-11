//! Tests of the shell scripts shipped in the release tarball and used by the
//! release workflow. They run in temporary directories only.

// The whole file is test code; clippy.toml's allow-in-tests only covers the
// #[test] functions, not the helpers they share.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Lay out a fake unpacked tarball: install.sh, desktop file, icon, binary.
fn fake_tarball(dir: &Path) {
    let linux = root().join("packaging/linux");
    fs::copy(linux.join("install.sh"), dir.join("install.sh")).unwrap();
    fs::copy(
        linux.join("io.github.paxel.multigit.desktop"),
        dir.join("io.github.paxel.multigit.desktop"),
    )
    .unwrap();
    fs::copy(root().join("assets/icon_256.png"), dir.join("icon_256.png")).unwrap();
    fs::write(dir.join("multigit"), "#!/bin/sh\necho fake\n").unwrap();
}

#[test]
fn install_script_puts_binary_entry_and_icon_in_place() {
    let tmp = tempfile::tempdir().unwrap();
    let unpacked = tmp.path().join("multigit-0.1.0");
    let home = tmp.path().join("home dir");
    fs::create_dir_all(&unpacked).unwrap();
    fs::create_dir_all(&home).unwrap();
    fake_tarball(&unpacked);

    let out = Command::new("sh")
        .arg(unpacked.join("install.sh"))
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bin = home.join(".local/bin/multigit");
    assert_eq!(fs::read_to_string(&bin).unwrap(), "#!/bin/sh\necho fake\n");
    let entry =
        fs::read_to_string(home.join(".local/share/applications/io.github.paxel.multigit.desktop"))
            .unwrap();
    assert!(
        entry.contains(&format!("\nExec=\"{}\"\n", bin.display())),
        "{entry}"
    );
    assert!(entry.contains("X-Multigit-Managed=true"));
    assert_eq!(
        fs::read(home.join(".local/share/icons/hicolor/256x256/apps/multigit.png")).unwrap(),
        fs::read(root().join("assets/icon_256.png")).unwrap()
    );
}

fn release_notes(version: &str, changelog: &Path) -> std::process::Output {
    Command::new("sh")
        .arg(root().join("packaging/release-notes.sh"))
        .arg(version)
        .arg(changelog)
        .output()
        .unwrap()
}

#[test]
fn release_notes_extract_the_version_section() {
    let tmp = tempfile::tempdir().unwrap();
    let changelog = tmp.path().join("CHANGELOG.md");
    fs::write(
        &changelog,
        "# Changelog\n\nintro\n\n## [0.2.0]\n\n### Added\n- New thing.\n\n---\n\nHistory link.\n",
    )
    .unwrap();
    let out = release_notes("0.2.0", &changelog);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "\n### Added\n- New thing.\n"
    );
    assert!(!release_notes("9.9.9", &changelog).status.success());
}

#[test]
fn current_changelog_has_notes_for_cargo_version() {
    let version = env!("CARGO_PKG_VERSION");
    let out = release_notes(version, &root().join("CHANGELOG.md"));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_bundle_has_binary_plist_and_icon() {
    let tmp = tempfile::tempdir().unwrap();
    let binary = tmp.path().join("multigit");
    fs::write(&binary, "#!/bin/sh\necho fake\n").unwrap();
    let out_dir = tmp.path().join("out dir");
    fs::create_dir_all(&out_dir).unwrap();

    let out = Command::new("sh")
        .arg(root().join("packaging/macos/bundle.sh"))
        .arg(&binary)
        .arg("1.2.3")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let contents = out_dir.join("multigit.app/Contents");
    assert_eq!(
        fs::read_to_string(contents.join("MacOS/multigit")).unwrap(),
        "#!/bin/sh\necho fake\n"
    );
    for license in ["LICENSE", "Fredoka-OFL.txt"] {
        assert!(
            contents.join("Resources").join(license).is_file(),
            "{license}"
        );
    }
    let plist = contents.join("Info.plist");
    let lint = Command::new("plutil")
        .arg("-lint")
        .arg(&plist)
        .output()
        .unwrap();
    assert!(
        lint.status.success(),
        "{}",
        String::from_utf8_lossy(&lint.stdout)
    );
    let verify = Command::new("codesign")
        .arg("--verify")
        .arg("--strict")
        .arg(out_dir.join("multigit.app"))
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let plist = fs::read_to_string(plist).unwrap();
    assert!(plist.contains("<string>1.2.3</string>"));
    assert!(!plist.contains("@VERSION@"));
    assert!(plist.contains("<string>io.github.paxel.multigit</string>"));
    let icns = fs::read(contents.join("Resources/multigit.icns")).unwrap();
    assert_eq!(&icns[0..4], b"icns");
}

#[test]
fn install_icon_script_installs_and_uninstalls_entry_and_icon() {
    let tmp = tempfile::tempdir().unwrap();
    let libexec = tmp.path().join("lib exec");
    let data = tmp.path().join("data home");
    fs::create_dir_all(&libexec).unwrap();
    let linux = root().join("packaging/linux");
    fs::copy(
        linux.join("install-icon.sh"),
        libexec.join("install-icon.sh"),
    )
    .unwrap();
    fs::write(
        libexec.join("io.github.paxel.multigit.desktop"),
        "[Desktop Entry]\nExec=/brew/opt/multigit/bin/multigit\n",
    )
    .unwrap();
    fs::copy(
        root().join("assets/icon_256.png"),
        libexec.join("icon_256.png"),
    )
    .unwrap();
    let run = |args: &[&str]| {
        let out = Command::new("sh")
            .arg(libexec.join("install-icon.sh"))
            .args(args)
            .env("XDG_DATA_HOME", &data)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };

    let entry = data.join("applications/io.github.paxel.multigit.desktop");
    let icon = data.join("icons/hicolor/256x256/apps/multigit.png");
    run(&[]);
    run(&[]);
    assert_eq!(
        fs::read_to_string(&entry).unwrap(),
        "[Desktop Entry]\nExec=/brew/opt/multigit/bin/multigit\n"
    );
    assert_eq!(
        fs::read(&icon).unwrap(),
        fs::read(root().join("assets/icon_256.png")).unwrap()
    );
    run(&["--uninstall"]);
    assert!(!entry.exists());
    assert!(!icon.exists());

    let bad = Command::new("sh")
        .arg(libexec.join("install-icon.sh"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn render_channels_fills_version_and_checksums() {
    let tmp = tempfile::tempdir().unwrap();
    let dist = tmp.path().join("dist");
    let out = tmp.path().join("out");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("multigit-1.2.3-x86_64-linux.tar.gz"), "linux").unwrap();
    fs::write(dist.join("multigit-1.2.3-macos-universal.zip"), "macos").unwrap();
    let render = |version: &str| {
        Command::new("sh")
            .arg(root().join("packaging/render-channels.sh"))
            .arg(version)
            .arg(&dist)
            .arg(&out)
            .output()
            .unwrap()
    };
    let result = render("1.2.3");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    // sha256 of "linux" and "macos".
    let formula = fs::read_to_string(out.join("multigit-formula.rb")).unwrap();
    assert!(formula.contains("version \"1.2.3\""));
    assert!(formula.contains("releases/download/v1.2.3/multigit-1.2.3-x86_64-linux.tar.gz"));
    assert!(
        formula.contains(
            "sha256 \"caf90169eefa5f807d577486b9f795ab86ae2983c5c20806cff959117e90af18\""
        )
    );
    assert!(!formula.contains("{{"));
    let cask = fs::read_to_string(out.join("multigit-cask.rb")).unwrap();
    assert!(cask.contains("version \"1.2.3\""));
    assert!(
        cask.contains(
            "sha256 \"a0c0485eff9c817deee7190edf264622b1677fc992920657411b36a6aed48f83\""
        )
    );
    assert!(!cask.contains("{{"));

    // A missing artifact or a bad version renders nothing.
    assert!(!render("9.9.9").status.success());
    assert!(!render("1.2.3/x").status.success());
}

/// A release may not go out without the licences: multigit's own and the
/// one of the font in the binary are in the Linux tarball and in the macOS
/// bundle (the bundle itself is checked on macOS above).
#[test]
fn release_archives_carry_the_licenses() {
    assert!(
        fs::read_to_string(root().join("LICENSE"))
            .unwrap()
            .starts_with("MIT License")
    );
    let font_license = fs::read_to_string(root().join("assets/fonts/Fredoka-OFL.txt")).unwrap();
    assert!(font_license.contains("SIL Open Font License"));
    for packer in [".github/workflows/release.yml", "packaging/macos/bundle.sh"] {
        let text = fs::read_to_string(root().join(packer)).unwrap();
        let copies = text
            .lines()
            .any(|l| l.contains("cp ") && l.contains("LICENSE") && l.contains("Fredoka-OFL.txt"));
        assert!(copies, "{packer} does not pack the licences");
    }
    let formula = fs::read_to_string(root().join("packaging/homebrew/formula.rb.tmpl")).unwrap();
    assert!(formula.contains("license \"MIT\""));
}

/// The About page's licence list names only crates of the lock file: after a
/// dependency update, `tool/licenses.sh` has to write it again.
#[test]
fn license_list_matches_the_lock_file() {
    let lock = fs::read_to_string(root().join("Cargo.lock")).unwrap();
    let mut locked = std::collections::HashSet::new();
    let mut name = "";
    for line in lock.lines() {
        if let Some(n) = line.strip_prefix("name = ") {
            name = n.trim_matches('"');
        } else if let Some(v) = line.strip_prefix("version = ") {
            locked.insert(format!("{name} {}", v.trim_matches('"')));
        }
    }
    let list = fs::read_to_string(root().join("assets/licenses.txt")).unwrap();
    let stale: Vec<&str> = list
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(krate, _)| krate)
        .filter(|krate| !locked.contains(*krate))
        .collect();
    assert!(stale.is_empty(), "run tool/licenses.sh: {stale:?}");
    assert!(list.lines().count() > 100);
}
