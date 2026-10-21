use super::*;

fn macho(minimum: u32) -> Vec<u8> {
    let mut bytes = vec![0; 56];
    for (offset, value) in [
        (0, 0xfeedfacfu32),
        (4, 0x01000007),
        (12, 2),
        (16, 1),
        (20, 24),
        (32, 0x32),
        (36, 24),
        (40, 1),
        (44, minimum),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[test]
fn macho_requires_intel_macos_and_a_big_sur_compatible_minimum() {
    assert!(validate_macho(&macho(0x000b0000)).is_ok());
    assert!(validate_macho(&macho(0x000a0d00)).is_ok());
    assert!(validate_macho(&macho(0x000b0100)).is_err());
    let mut arm = macho(0x000b0000);
    arm[4..8].copy_from_slice(&0x0100000cu32.to_le_bytes());
    assert!(validate_macho(&arm).is_err());
    let mut ios = macho(0x000b0000);
    ios[40..44].copy_from_slice(&2u32.to_le_bytes());
    assert!(validate_macho(&ios).is_err());
    assert!(validate_macho(&macho(0x000b0000)[..40]).is_err());
}

fn archive(entries: &[(&str, u32, u32)]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut directory = Vec::new();
    for (name, mode, expanded) in entries {
        let local = data.len() as u32;
        let mut header = vec![0u8; 30];
        header[0..4].copy_from_slice(&0x04034b50u32.to_le_bytes());
        header[22..26].copy_from_slice(&expanded.to_le_bytes());
        header[26..28].copy_from_slice(&(name.len() as u16).to_le_bytes());
        data.extend(header);
        data.extend(name.as_bytes());
        let mut entry = vec![0u8; 46];
        entry[0..4].copy_from_slice(&0x02014b50u32.to_le_bytes());
        entry[24..28].copy_from_slice(&expanded.to_le_bytes());
        entry[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
        entry[38..42].copy_from_slice(&(mode << 16).to_le_bytes());
        entry[42..46].copy_from_slice(&local.to_le_bytes());
        directory.extend(entry);
        directory.extend(name.as_bytes());
    }
    let offset = data.len() as u32;
    let size = directory.len() as u32;
    data.extend(directory);
    let mut end = vec![0u8; 22];
    end[0..4].copy_from_slice(&0x06054b50u32.to_le_bytes());
    end[8..10].copy_from_slice(&(entries.len() as u16).to_le_bytes());
    end[10..12].copy_from_slice(&(entries.len() as u16).to_le_bytes());
    end[12..16].copy_from_slice(&size.to_le_bytes());
    end[16..20].copy_from_slice(&offset.to_le_bytes());
    data.extend(end);
    data
}
fn required_entries() -> Vec<(&'static str, u32, u32)> {
    vec![
        ("Turner.app/Contents/Info.plist", 0o100644, 0),
        ("Turner.app/Contents/MacOS/guitar-tuner-egui", 0o100755, 0),
    ]
}
#[test]
fn zip_rejects_traversal_symlinks_case_aliases_and_excessive_expansion() {
    assert!(validate_zip(&archive(&required_entries())).is_ok());
    for name in [
        "../outside",
        "/tmp/outside",
        "Turner.app/../outside",
        "Turner.app/Contents/../../escape",
        "Turner.app/Contents\\outside",
        "Other.app/file",
    ] {
        let mut entries = required_entries();
        entries.push((name, 0o100644, 0));
        assert!(validate_zip(&archive(&entries)).is_err(), "{name}");
    }
    let mut entries = required_entries();
    entries.push(("Turner.app/Contents/link", 0o120777, 0));
    assert!(validate_zip(&archive(&entries)).is_err());
    let mut entries = required_entries();
    entries.push(("Turner.app/Contents/INFO.PLIST", 0o100644, 0));
    assert!(validate_zip(&archive(&entries)).is_err());
    let mut entries = required_entries();
    entries[0].2 = MAX_EXPANDED as u32 + 1;
    assert!(validate_zip(&archive(&entries)).is_err());
}
#[test]
fn zip_rejects_local_directory_name_mismatch_and_hidden_prefix() {
    let mut mismatch = archive(&required_entries());
    mismatch[30] = b'X';
    assert!(validate_zip(&mismatch).is_err());
    let mut prefixed = vec![b'X'];
    prefixed.extend(archive(&required_entries()));
    assert!(validate_zip(&prefixed).is_err());
    let mut conflict = required_entries();
    conflict.push(("Turner.app/Contents/Info.plist/child", 0o100644, 0));
    assert!(validate_zip(&archive(&conflict)).is_err());
}
#[test]
fn manifest_and_urls_bind_revision_identity_and_exact_repository() {
    let tag = "turner-v0.1.13-r4";
    let asset = "Turner-v0.1.13-r4-macos-x86_64.app.zip";
    let url = asset_url(tag, asset);
    let hash = "ab".repeat(32);
    let mut manifest = serde_json::json!({"schema_version":1,"app_id":"turner","channel":"stable","version":"0.1.13","revision":4,"tag":tag,"platform":"macos","arch":"x86_64","minimum_macos":"11.0","asset":asset,"url":url,"sha256":hash,"bundle_id":BUNDLE_ID,"executable":EXECUTABLE});
    assert!(validate_manifest(&manifest, "0.1.13", 4, tag, asset, &url).is_ok());
    manifest["bundle_id"] = Value::String("some.other.app".into());
    assert!(validate_manifest(&manifest, "0.1.13", 4, tag, asset, &url).is_err());
    let mut release = serde_json::json!({"assets":[{"name":asset,"browser_download_url":url,"size":10,"state":"uploaded"}]});
    assert!(release_asset(&release, tag, asset, MAX_ZIP).is_ok());
    for bad_url in [
        "http://github.com/Tangram-Space/updates/x",
        "https://github.com.attacker.invalid/file",
        "https://github.com/another/repo/releases/download/turner-v0.1.13-r4/file.zip",
        "https://github.com/Chevosky/tuner-compact/releases/download/compact-v0.1.13-r4/Tuner-Compact-v0.1.13-r4-macos-x86_64.app.zip",
        "https://github.com/Tangram-Space/updates/releases/download/airplay-v1.0.0/AirPlay.zip",
    ] {
        release["assets"][0]["browser_download_url"] = Value::String(bad_url.into());
        assert!(release_asset(&release, tag, asset, MAX_ZIP).is_err());
    }
    for bad_tag in [
        "turner-v0.1.13-r0",
        "turner-v0.1.13-r4/../../x",
        "v0.1.13-r4",
        "turner-v0.1.13-r$(touch file)",
        "compact-v0.1.13-r4",
        "airplay-v0.1.13-r4",
    ] {
        assert!(parse_tag(bad_tag).is_err());
        assert!(release_api_url(bad_tag).is_err());
    }
}

fn turner_feed() -> Value {
    serde_json::json!({
        "schema_version": 1,
        "app_id": "turner",
        "channel": "stable",
        "version": "0.1.13",
        "revision": 4,
        "tag": "turner-v0.1.13-r4",
        "platform": "macos",
        "arch": "x86_64",
        "minimum_macos": "11.0",
        "asset": "Turner-v0.1.13-r4-macos-x86_64.app.zip",
        "url": "https://github.com/Tangram-Space/updates/releases/download/turner-v0.1.13-r4/Turner-v0.1.13-r4-macos-x86_64.app.zip",
        "sha256": "ab".repeat(32),
        "bundle_id": "com.tuner.guitar.compact",
        "executable": "guitar-tuner-egui"
    })
}

#[test]
fn per_app_feed_selects_an_exact_release_even_when_other_apps_publish_newer() {
    let selected = parse_feed(&turner_feed()).unwrap();
    assert_eq!(selected.info.revision, 4);
    assert_eq!(
        release_api_url(&selected.tag).unwrap(),
        "https://api.github.com/repos/Tangram-Space/updates/releases/tags/turner-v0.1.13-r4"
    );
    let mut release = serde_json::json!({
        "tag_name": "turner-v0.1.13-r4", "draft": false, "prerelease": false
    });
    assert!(validate_selected_release(&release, &selected.tag).is_ok());
    // A global latest release could legitimately be AirPlay. It can never be
    // substituted for the Turner release announced by this app's own feed.
    release["tag_name"] = Value::String("airplay-v9.0.0".into());
    assert!(validate_selected_release(&release, &selected.tag).is_err());
    release["tag_name"] = Value::String("turner-v0.1.13-r5".into());
    assert!(validate_selected_release(&release, &selected.tag).is_err());
    release["tag_name"] = Value::String(selected.tag.clone());
    release["draft"] = Value::Bool(true);
    assert!(validate_selected_release(&release, &selected.tag).is_err());
    release["draft"] = Value::Bool(false);
    release["prerelease"] = Value::Bool(true);
    assert!(validate_selected_release(&release, &selected.tag).is_err());
}

#[test]
fn feed_rejects_cross_app_channel_identity_and_legacy_release_metadata() {
    for (key, wrong) in [
        ("app_id", "airplay"),
        ("channel", "beta"),
        ("bundle_id", "some.airplay.app"),
        ("executable", "AirPlay"),
        ("version", "0.1.14"),
        ("tag", "compact-v0.1.13-r4"),
        ("arch", "aarch64"),
        ("minimum_macos", "12.0"),
        ("asset", "Tuner-Compact-v0.1.13-r4-macos-x86_64.app.zip"),
        (
            "url",
            "https://github.com/Tangram-Space/updates/releases/latest/download/AirPlay.zip",
        ),
    ] {
        let mut feed = turner_feed();
        feed[key] = Value::String(wrong.into());
        assert!(parse_feed(&feed).is_err(), "{key}");
    }
    let mut wrong_revision = turner_feed();
    wrong_revision["revision"] = Value::from(5);
    assert!(parse_feed(&wrong_revision).is_err());
    let mut missing_identity = turner_feed();
    missing_identity.as_object_mut().unwrap().remove("app_id");
    assert!(parse_feed(&missing_identity).is_err());
}

#[test]
fn stable_feed_and_release_manifest_must_describe_the_identical_artifact() {
    let feed = turner_feed();
    let formatted = serde_json::to_vec_pretty(&feed).unwrap();
    assert!(validate_manifest_pair(&feed, &json(&formatted).unwrap()).is_ok());
    let mut altered_hash = feed.clone();
    altered_hash["sha256"] = Value::String("cd".repeat(32));
    assert!(parse_feed(&altered_hash).is_ok());
    assert!(validate_manifest_pair(&feed, &altered_hash).is_err());
    let mut altered_provenance = feed.clone();
    altered_provenance["source_sha"] = Value::String("0123456789abcdef".into());
    assert!(validate_manifest_pair(&feed, &altered_provenance).is_err());
}

#[test]
fn legacy_installed_path_is_accepted_but_downloaded_archive_requires_turner_root() {
    for name in ["Turner.app", "Tuner Compact.app", "My Turner.app"] {
        let bundle = PathBuf::from("/Applications").join(name);
        let executable = bundle.join("Contents/MacOS/guitar-tuner-egui");
        assert_eq!(bundle_from_executable(&executable).unwrap(), bundle);
    }
    assert!(bundle_from_executable(Path::new("/tmp/guitar-tuner-egui")).is_err());
    assert!(bundle_from_executable(Path::new(
        "/Applications/Turner.app/Contents/MacOS/OtherApp"
    ))
    .is_err());
    let old_archive = archive(&[
        ("Tuner Compact.app/Contents/Info.plist", 0o100644, 0),
        (
            "Tuner Compact.app/Contents/MacOS/guitar-tuner-egui",
            0o100755,
            0,
        ),
    ]);
    assert!(validate_zip(&old_archive).is_err());
    assert!(validate_zip(&archive(&required_entries())).is_ok());
}
#[test]
fn checksum_must_name_the_exact_archive_once() {
    let hash = "12".repeat(32);
    assert_eq!(
        parse_checksum(format!("{hash}  app.zip\n").as_bytes(), "app.zip").unwrap(),
        hash
    );
    assert!(parse_checksum(format!("{hash}  wrong.zip\n").as_bytes(), "app.zip").is_err());
    assert!(parse_checksum(
        format!("{hash}  app.zip\n{hash}  app.zip\n").as_bytes(),
        "app.zip"
    )
    .is_err());
}
fn test_job(
    service: &mut UpdateService,
) -> (mpsc::Sender<Event>, mpsc::Receiver<()>, Arc<AtomicBool>) {
    let (sender, events) = mpsc::channel();
    let (commit, received) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    service.job = Some(Job {
        events,
        commit: Some(commit),
        cancelled: cancelled.clone(),
        handoff_authorized: false,
    });
    (sender, received, cancelled)
}
#[test]
fn cancelling_preparation_revokes_commit_and_discards_stale_completion() {
    let mut service = UpdateService::default();
    service.status = UpdateStatus::Preparing;
    let (sender, commit, token) = test_job(&mut service);
    sender.send(Event::Prepared).unwrap();
    service.cancel();
    assert!(token.load(Ordering::Acquire));
    assert!(matches!(
        commit.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    assert!(!service.poll(&egui::Context::default()));
    assert!(matches!(service.status(), UpdateStatus::Cancelled));
    assert!(sender.send(Event::Restart).is_err());
}
#[test]
fn restart_requires_prepared_ui_handoff_and_committed_helper_event() {
    let mut service = UpdateService::default();
    service.status = UpdateStatus::Preparing;
    let (sender, commit, token) = test_job(&mut service);
    sender.send(Event::Prepared).unwrap();
    assert!(!service.poll(&egui::Context::default()));
    assert!(matches!(service.status(), UpdateStatus::ReadyToRestart));
    assert!(commit.try_recv().is_ok());
    service.cancel();
    assert!(!token.load(Ordering::Acquire));
    sender.send(Event::Restart).unwrap();
    assert!(service.poll(&egui::Context::default()));
    assert!(!service.poll(&egui::Context::default()));
}
#[test]
fn downloaded_hash_mismatch_is_detectable_and_cancellation_stops_hashing() {
    let directory = PrivateDir::new(&std::env::temp_dir()).unwrap();
    let file = directory.path.join("data");
    fs::write(&file, b"abc").unwrap();
    assert_eq!(
        hash_file(&file, &AtomicBool::new(false)).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(matches!(
        hash_file(&file, &AtomicBool::new(true)),
        Err(Failure::Cancelled)
    ));
}

#[cfg(unix)]
fn fake_bundle(path: &Path, identity: &str) {
    fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
    fs::write(path.join("Contents/Info.plist"), identity).unwrap();
    let executable = path.join("Contents/MacOS/guitar-tuner-egui");
    fs::write(&executable, b"fixture").unwrap();
    fs::set_permissions(executable, fs::Permissions::from_mode(0o700)).unwrap();
}
#[cfg(unix)]
#[test]
fn installer_keeps_backup_on_success_and_rolls_back_failed_launch_with_literal_paths() {
    for reject_new in [false, true] {
        let directory = PrivateDir::new(&std::env::temp_dir()).unwrap();
        let target = directory.path.join("Tuner ' $(touch SHOULD_NOT_EXIST).app");
        let stage = directory.path.join("stage");
        fs::create_dir(&stage).unwrap();
        let next = stage.join("Next.app");
        let backup = stage.join("Previous.app");
        fake_bundle(&target, "old");
        fake_bundle(&next, "new");
        fs::write(stage.join("commit"), b"install\n").unwrap();
        let buddy = directory.path.join("plistbuddy");
        fs::write(
            &buddy,
            r#"#!/bin/bash
identity=$(/bin/cat "$3")
case "$2" in
  'Print :CFBundleIdentifier') printf 'com.tuner.guitar.compact\n';;
  'Print :CFBundleExecutable') printf 'guitar-tuner-egui\n';;
  'Print :CFBundleShortVersionString') printf '0.1.13\n';;
  'Print :CFBundleVersion') if [[ "$identity" == old ]]; then printf '3\n'; else printf '4\n'; fi;;
  *) exit 1;;
esac
"#,
        )
        .unwrap();
        let opener = directory.path.join("open");
        fs::write(
            &opener,
            r#"#!/bin/bash
identity=$(/bin/cat "$2/Contents/Info.plist")
printf '%s\n' "$identity" >> "$TUNER_TEST_OPEN_LOG"
[[ "$TUNER_TEST_REJECT_NEW" != 1 || "$identity" != new ]]
"#,
        )
        .unwrap();
        for path in [&buddy, &opener] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let script = stage.join("install.sh");
        fs::write(
            &script,
            INSTALLER_SCRIPT
                .replace("/usr/libexec/PlistBuddy", "\"$TUNER_TEST_PLISTBUDDY\"")
                .replace("/usr/bin/open", "\"$TUNER_TEST_OPEN\""),
        )
        .unwrap();
        let status = Command::new("/bin/bash")
            .arg(&script)
            .arg(&target)
            .arg(&next)
            .arg(&backup)
            .arg("999999999")
            .arg(&stage)
            .arg("0.1.13")
            .arg("3")
            .arg("0.1.13")
            .arg("4")
            .env("TUNER_TEST_PLISTBUDDY", &buddy)
            .env("TUNER_TEST_OPEN", &opener)
            .env("TUNER_TEST_OPEN_LOG", stage.join("opens.log"))
            .env("TUNER_TEST_REJECT_NEW", if reject_new { "1" } else { "0" })
            .current_dir(&directory.path)
            .status()
            .unwrap();
        assert_eq!(status.success(), !reject_new);
        assert!(!directory.path.join("SHOULD_NOT_EXIST").exists());
        if reject_new {
            assert_eq!(
                fs::read_to_string(target.join("Contents/Info.plist")).unwrap(),
                "old"
            );
            assert_eq!(
                fs::read_to_string(stage.join("opens.log")).unwrap(),
                "new\nold\n"
            );
        } else {
            assert_eq!(
                fs::read_to_string(target.join("Contents/Info.plist")).unwrap(),
                "new"
            );
            assert_eq!(
                fs::read_to_string(backup.join("Contents/Info.plist")).unwrap(),
                "old"
            );
        }
    }
}

#[test]
fn real_release_fixture_when_explicitly_provided() {
    if let Some(path) = std::env::var_os("TUNER_QA_RELEASE_ZIP") {
        validate_zip(&fs::read(path).unwrap()).unwrap();
    }
    if let Some(path) = std::env::var_os("TUNER_QA_RELEASE_MACHO") {
        validate_macho(&fs::read(path).unwrap()).unwrap();
    }
}
