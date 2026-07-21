use aifficator_core::planner::{build_conversion_plan, PlanAction, PlanOptions};
use aifficator_core::rekordbox::parse_rekordbox_xml;
use aifficator_core::validation::{validate_library, IssueCode};
use aifficator_core::{conversion::ffmpeg_args, conversion::ConversionSettings};
use aifficator_core::{
    exporter::export_replacement_xml, exporter::export_track_ratings_xml,
    exporter::export_with_new_playlist_xml, exporter::path_to_rekordbox_location,
    exporter::ExportTrackReplacement,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use url::Url;

#[test]
fn parses_tracks_and_playlists() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_path = temp_dir.path().join("Track One.flac");
    fs::write(
        &source_path,
        b"not real audio but enough for file validation",
    )
    .unwrap();
    let source_url = Url::from_file_path(&source_path).unwrap().to_string();

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="rekordbox" Version="7.2.3" Company="AlphaTheta"/>
  <COLLECTION Entries="2">
    <TRACK TrackID="1" Name="Track One" Artist="Artist" Kind="FLAC File" Location="{source_url}"/>
    <TRACK TrackID="2" Name="Missing" Artist="Artist" Kind="MP3 File" Location="file://localhost/tmp/missing-track.mp3"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT" Count="1">
      <NODE Name="Set" Type="1" KeyType="0" Entries="2">
        <TRACK Key="1"/>
        <TRACK Key="2"/>
      </NODE>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>"#
    );

    let library = parse_rekordbox_xml(&xml).unwrap();
    assert_eq!(library.tracks.len(), 2);
    assert_eq!(library.playlists_flat().len(), 2);

    let report = validate_library(&library);
    assert_eq!(report.tracks_total, 2);
    assert_eq!(report.convert_candidates, 2);
    assert_eq!(report.missing_files, 1);
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == IssueCode::FileNotFound));
}

#[test]
fn builds_conversion_plan_for_selected_playlist() {
    let temp_dir = tempfile::tempdir().unwrap();
    let source_path = temp_dir.path().join("Track One.flac");
    fs::write(
        &source_path,
        b"not real audio but enough for file validation",
    )
    .unwrap();
    let source_url = Url::from_file_path(&source_path).unwrap().to_string();

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <COLLECTION Entries="1">
    <TRACK TrackID="1" Name="Track One" Artist="Artist" Kind="FLAC File" Location="{source_url}"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT" Count="1">
      <NODE Name="Set" Type="1" KeyType="0" Entries="1">
        <TRACK Key="1"/>
      </NODE>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>"#
    );

    let library = parse_rekordbox_xml(&xml).unwrap();
    let plan = build_conversion_plan(
        &library,
        PlanOptions {
            playlist_paths: vec!["ROOT/Set".to_string()],
            reuse_existing: true,
        },
    );

    assert_eq!(plan.unique_tracks_total, 1);
    assert_eq!(plan.convert_total, 1);
    assert_eq!(plan.items[0].action, PlanAction::Convert);
    assert!(plan.items[0]
        .target_path
        .as_ref()
        .unwrap()
        .ends_with("converted/Track One.aiff"));
}

#[test]
fn ffmpeg_args_use_compatible_aiff_defaults() {
    let args = ffmpeg_args(
        Path::new("/music/source.flac"),
        Path::new("/music/converted/source.aiff"),
        &ConversionSettings::default(),
    );

    assert!(args.windows(2).any(|pair| pair == ["-c:a", "pcm_s16be"]));
    assert!(args.windows(2).any(|pair| pair == ["-ar", "44100"]));
    assert!(args.windows(2).any(|pair| pair == ["-ac", "2"]));
    assert!(args.iter().any(|arg| arg == "-n"));
}

#[test]
fn export_replacement_xml_rewrites_track_location_and_preserves_children() {
    let target_location = path_to_rekordbox_location(Path::new("/tmp/converted/Track One.aiff"))
        .expect("valid file URL");
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <COLLECTION Entries="1">
    <TRACK TrackID="1" Name="Track One" Kind="FLAC File" Location="file://localhost/tmp/Track%20One.flac">
      <TEMPO Inizio="0.025" Bpm="128.00" Metro="4/4" Battito="1"/>
    </TRACK>
  </COLLECTION>
</DJ_PLAYLISTS>"#;
    let replacements = BTreeMap::from([(
        "1".to_string(),
        ExportTrackReplacement {
            location: target_location,
            kind: "AIFF File".to_string(),
            size: Some(1234),
            sample_rate: Some(44_100),
            bit_rate: Some(1411),
        },
    )]);

    let exported = export_replacement_xml(xml, &replacements).unwrap();

    assert!(exported.contains("Kind=\"AIFF File\""));
    assert!(exported.contains("Location=\"file://localhost/tmp/converted/Track%20One.aiff\""));
    assert!(exported.contains("<TEMPO Inizio=\"0.025\" Bpm=\"128.00\""));
}

#[test]
fn export_track_ratings_overrides_only_internal_ratings() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <COLLECTION Entries="3">
    <TRACK TrackID="1" Name="Five Stars" Rating="51"/>
    <TRACK TrackID="2" Name="Cleared" Rating="204"/>
    <TRACK TrackID="3" Name="Preserved" Rating="153"/>
  </COLLECTION>
</DJ_PLAYLISTS>"#;
    let ratings = BTreeMap::from([("1".to_string(), 5_u8), ("2".to_string(), 0_u8)]);

    let exported = export_track_ratings_xml(xml, &ratings).expect("export ratings");
    let parsed = parse_rekordbox_xml(&exported).expect("parse rated XML");
    let by_id = parsed.track_by_id();

    assert_eq!(by_id["1"].attributes.get("Rating").map(String::as_str), Some("255"));
    assert_eq!(by_id["2"].attributes.get("Rating").map(String::as_str), Some("0"));
    assert_eq!(by_id["3"].attributes.get("Rating").map(String::as_str), Some("153"));
}

#[test]
fn export_with_new_playlist_xml_appends_rau_studio_playlist() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <COLLECTION Entries="2">
    <TRACK TrackID="1" Name="One" Kind="AIFF File"/>
    <TRACK TrackID="2" Name="Two" Kind="AIFF File"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT" Count="0"/>
  </PLAYLISTS>
</DJ_PLAYLISTS>"#;

    let exported =
        export_with_new_playlist_xml(xml, "Generated Set", &["1".to_string(), "2".to_string()])
            .unwrap();

    assert!(exported.contains("Name=\"Rau Studio\""));
    assert!(exported.contains("Name=\"Generated Set\""));
    assert!(exported.contains("Entries=\"2\""));
    assert!(exported.contains("<TRACK Key=\"1\"/>"));
    assert!(exported.contains("<TRACK Key=\"2\"/>"));
    assert!(exported.contains("<TRACK TrackID=\"1\" Name=\"One\" Kind=\"AIFF File\"/>"));

    let parsed = parse_rekordbox_xml(&exported).unwrap();
    assert_eq!(parsed.playlists.len(), 1);
    let root = &parsed.playlists[0];
    assert_eq!(root.name, "ROOT");
    assert_eq!(root.count, Some(1));
    assert_eq!(root.children.len(), 1);
    let rau_studio_folder = &root.children[0];
    assert_eq!(rau_studio_folder.name, "Rau Studio");
    assert_eq!(rau_studio_folder.children.len(), 1);
    assert_eq!(rau_studio_folder.children[0].name, "Generated Set");
    assert_eq!(
        rau_studio_folder.children[0].track_keys,
        vec!["1".to_string(), "2".to_string()]
    );
}

#[test]
fn export_with_new_playlist_xml_increments_and_preserves_existing_root() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="rekordbox" Version="7.2.3" Company="AlphaTheta"/>
  <COLLECTION Entries="1">
    <TRACK TrackID="1" Name="One" Kind="AIFF File"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT" Count="1">
      <NODE Name="Existing Set" Type="1" KeyType="0" Entries="1">
        <TRACK Key="1"/>
      </NODE>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>"#;

    let exported = export_with_new_playlist_xml(xml, "Generated Set", &["1".to_string()])
        .expect("export playlist");
    let parsed = parse_rekordbox_xml(&exported).expect("parse exported XML");

    assert_eq!(parsed.playlists.len(), 1);
    let root = &parsed.playlists[0];
    assert_eq!(root.name, "ROOT");
    assert_eq!(root.count, Some(2));
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[0].name, "Existing Set");
    assert_eq!(root.children[1].name, "Rau Studio");
    assert_eq!(root.children[1].children[0].name, "Generated Set");
}
